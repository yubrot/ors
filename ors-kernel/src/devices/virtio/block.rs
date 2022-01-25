use super::{Buffer, Configuration, VirtQueue};
use crate::cpu::Cpu;
use crate::devices::pci;
use crate::interrupts::virtio_block_irq;
use crate::sync::spin::Spin;
use crate::task;
use core::mem;
use core::sync::atomic::{fence, Ordering};
use derive_new::new;
use heapless::Vec;
use log::trace;
use spin::Once;

static BLOCKS: Once<Vec<Block, 8>> = Once::new();

pub fn initialize() {
    BLOCKS.call_once(|| {
        trace!("INITIALIZING VirtIO Blocks");
        unsafe { Block::scan::<8>() }
    });
}

pub fn list() -> &'static Vec<Block, 8> {
    BLOCKS
        .get()
        .expect("block::list is called before block::initialize")
}

#[derive(Debug)]
pub struct Block {
    configuration: Configuration,
    requestq: Spin<VirtQueue<Option<task::WaitChannel>>>,
}

impl Block {
    unsafe fn scan<const N: usize>() -> Vec<Self, N> {
        let mut blocks = Vec::new();

        for device in pci::devices() {
            if device.is_virtio() && device.subsystem_id() == 0x02 {
                match Block::from_pci_device(*device, blocks.len()) {
                    Ok(block) => match blocks.push(block) {
                        Ok(()) => {}
                        Err(block) => {
                            // FIXME: To remove mem::forget, we need to reset the device
                            mem::forget(block);
                            trace!("virtio: More than {} blocks are unsupported", N);
                        }
                    },
                    Err(msg) => trace!("virtio: Failed to initialize block: {}", msg),
                }
            }
        }

        blocks
    }

    unsafe fn from_pci_device(device: pci::Device, index: usize) -> Result<Self, &'static str> {
        if let Some(msi_x) = device.msi_x() {
            if msi_x.table().len() == 0 {
                return Err("MSI-X support does not have enough table entries");
            }

            let bsp = Cpu::boot_strap().lapic_id().unwrap();
            let irq = virtio_block_irq(index).ok_or("IRQ numbers exhausted")?;
            msi_x.table().entry(0).enable(bsp, irq); // for requestq
            msi_x.enable();
        } else {
            // Interrupts other than MSI-X is not implemented
            return Err("MSI-X unsupported");
        }

        let configuration = Configuration::from_pci_device(device)?;
        configuration.initialize(Self::negotiate)?;
        let requestq = Spin::new(VirtQueue::new(configuration, 0, Some(0))?);
        configuration.set_driver_ok();

        Ok(Self {
            configuration,
            requestq,
        })
    }

    /// Capacity of the device (expressed in `Self::SECTOR_SIZE` sectors)
    pub fn capacity(&self) -> u64 {
        let lower = unsafe { self.configuration.read_device_specific::<u32>(0x0) } as u64;
        let upper = unsafe { self.configuration.read_device_specific::<u32>(0x4) } as u64;
        lower | (upper << 32)
    }

    fn check_capacity(&self, sector: u64, len: usize) -> Result<(), Error> {
        let num_additional_sectors = (len.max(1) - 1) / Self::SECTOR_SIZE;
        if sector + (num_additional_sectors as u64) < self.capacity() {
            Ok(())
        } else {
            Err(Error::OutOfRange)
        }
    }

    fn request(
        &self,
        header: RequestHeader,
        body: Buffer<Option<task::WaitChannel>>,
    ) -> Result<(), Error> {
        let mut footer = RequestFooter::new(0);
        let complete_channel = task::WaitChannel::from_ptr(&footer);

        let mut buffers = [
            Buffer::from_ref(&header, None).unwrap(),
            body,
            Buffer::from_ref_mut(&mut footer, Some(complete_channel)).unwrap(),
        ]
        .into_iter();

        let mut requestq = self.requestq.lock();
        loop {
            match requestq.transfer(buffers) {
                Ok(()) => break,
                Err(b) => {
                    buffers = b;
                    task::scheduler().block(self.queue_wait_channel(), None, requestq);
                    requestq = self.requestq.lock();
                }
            }
        }
        unsafe { self.configuration.set_queue_notify(0) };

        task::scheduler().block(complete_channel, None, requestq);
        fence(Ordering::SeqCst);
        footer.into_result()
    }

    fn queue_wait_channel(&self) -> task::WaitChannel {
        task::WaitChannel::from_ptr(self)
    }

    /// Read data from this device.
    pub fn read(&self, sector: u64, buf: &mut [u8]) -> Result<(), Error> {
        self.check_capacity(sector, buf.len())?;
        let header = RequestHeader::new(RequestHeader::IN, 0, sector);
        let body = Buffer::from_bytes_mut(buf, None).unwrap();
        self.request(header, body)
    }

    /// Write data into this device.
    pub fn write(&self, sector: u64, buf: &[u8]) -> Result<(), Error> {
        self.check_capacity(sector, buf.len())?;
        let header = RequestHeader::new(RequestHeader::OUT, 0, sector);
        let body = Buffer::from_bytes(buf, None).unwrap();
        self.request(header, body)
    }

    /// Collect the processed requests.
    /// This method is supposed to be called from Used Buffer Notification (interrupt).
    pub fn collect(&self) {
        let mut requestq = self.requestq.lock();
        requestq.collect(|chan| {
            if let Some(chan) = chan {
                task::scheduler().release(chan);
            }
        });
        task::scheduler().release(self.queue_wait_channel());
    }

    fn negotiate(features: u32) -> u32 {
        // TODO: Understand the detailed semantics of these features
        // Currently we only support features that are enabled in xv6-riscv
        const RO: u32 = 1 << 5;
        const SCSI: u32 = 1 << 7;
        const CONFIG_WCE: u32 = 1 << 11;
        const MQ: u32 = 1 << 12;
        const ANY_LAYOUT: u32 = 1 << 27;
        features & !RO & !SCSI & !CONFIG_WCE & !MQ & !ANY_LAYOUT
    }

    pub const SECTOR_SIZE: usize = 512;
}

unsafe impl Sync for Block {}

unsafe impl Send for Block {}

#[derive(PartialEq, Eq, PartialOrd, Ord, Debug, Clone, Copy)]
#[non_exhaustive]
pub enum Error {
    Io,
    Unsupported,
    OutOfRange,
    Unknown,
}

#[repr(C)]
#[derive(Debug, new)]
struct RequestHeader {
    ty: u32,
    _reserved: u32,
    sector: u64,
}

impl RequestHeader {
    const IN: u32 = 0;
    const OUT: u32 = 1;
}

#[repr(C)]
#[derive(Debug, new)]
struct RequestFooter {
    status: u8,
}

impl RequestFooter {
    fn into_result(self) -> Result<(), Error> {
        match self.status {
            Self::STATUS_OK => Ok(()),
            Self::STATUS_IOERR => Err(Error::Io),
            Self::STATUS_UNSUPP => Err(Error::Unsupported),
            _ => Err(Error::Unknown),
        }
    }

    const STATUS_OK: u8 = 0;
    const STATUS_IOERR: u8 = 1;
    const STATUS_UNSUPP: u8 = 2;
}
