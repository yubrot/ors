#![no_std]
#![no_main]
#![feature(abi_efiapi)]
#![feature(vec_into_raw_parts)]

#[macro_use]
extern crate alloc;

#[macro_use]
mod fs;

use alloc::vec::Vec;
use core::{mem, slice};
use goblin::elf;
use log::info;
use ors_common::{asm, frame_buffer, memory_map};
use uefi::prelude::*;
use uefi::proto::console::gop::{GraphicsOutput, PixelFormat};
use uefi::table::boot::{AllocateType, MemoryDescriptor, MemoryType};
use uefi::table::Runtime;

const UEFI_PAGE_SIZE: usize = 0x1000;

#[entry]
fn efi_main(image: Handle, mut st: SystemTable<Boot>) -> Status {
    uefi_services::init(&mut st).unwrap_success();

    st.stdout().reset(false).unwrap_success();

    info!("dump_memory_map");
    dump_memory_map("memmap", image, &st);

    info!("load_kernel");
    let entry_point_addr = load_kernel("ors-kernel.elf", image, &st);

    info!("entry_point_addr = 0x{:x}", entry_point_addr);
    let entry_point: extern "sysv64" fn(&frame_buffer::FrameBuffer, &memory_map::MemoryMap) =
        unsafe { mem::transmute(entry_point_addr) };

    info!("get_frame_buffer_config");
    let frame_buffer = get_frame_buffer(st.boot_services());

    info!("exit_boot_services");
    let (_st, memory_map) = exit_boot_services(image, st);

    entry_point(&frame_buffer, &memory_map);

    loop {
        asm::hlt()
    }
}

fn dump_memory_map(path: &str, image: Handle, st: &SystemTable<Boot>) {
    let enough_mmap_size =
        st.boot_services().memory_map_size() + 8 * mem::size_of::<MemoryDescriptor>();
    let mut mmap_buf = vec![0; enough_mmap_size];
    let (_, descriptors) = st
        .boot_services()
        .memory_map(&mut mmap_buf)
        .unwrap_success();

    let mut root_dir = fs::open_root_dir(image, st.boot_services());
    let mut file = fs::create_file(&mut root_dir, path);
    fwriteln!(
        file,
        "Index, Type, Type(name), PhysicalStart, NumberOfPages, Attribute"
    );
    for (i, d) in descriptors.enumerate() {
        fwriteln!(
            file,
            "{}, {:x}, {:?}, {:08x}, {:x}, {:x}",
            i,
            d.ty.0,
            d.ty,
            d.phys_start,
            d.page_count,
            d.att.bits() & 0xfffff
        );
    }
}

fn load_kernel(path: &str, image: Handle, st: &SystemTable<Boot>) -> usize {
    let mut root_dir = fs::open_root_dir(image, st.boot_services());
    let mut file = fs::open_file(&mut root_dir, path);
    let buf = fs::read_file_to_vec(&mut file);
    load_elf(&buf, st)
}

fn load_elf(src: &[u8], st: &SystemTable<Boot>) -> usize {
    let elf = elf::Elf::parse(&src).expect("Failed to parse ELF");

    let mut dest_start = usize::MAX;
    let mut dest_end = 0;
    for ph in elf.program_headers.iter() {
        if ph.p_type != elf::program_header::PT_LOAD {
            continue;
        }
        dest_start = dest_start.min(ph.p_vaddr as usize);
        dest_end = dest_end.max((ph.p_vaddr + ph.p_memsz) as usize);
    }

    st.boot_services()
        .allocate_pages(
            AllocateType::Address(dest_start),
            MemoryType::LOADER_DATA,
            (dest_end - dest_start + UEFI_PAGE_SIZE - 1) / UEFI_PAGE_SIZE,
        )
        .expect_success("Failed to allocate pages for kernel");

    for ph in elf.program_headers.iter() {
        if ph.p_type != elf::program_header::PT_LOAD {
            continue;
        }
        let ofs = ph.p_offset as usize;
        let fsize = ph.p_filesz as usize;
        let msize = ph.p_memsz as usize;
        let dest = unsafe { slice::from_raw_parts_mut(ph.p_vaddr as *mut u8, msize) };
        dest[..fsize].copy_from_slice(&src[ofs..ofs + fsize]);
        dest[fsize..].fill(0);
    }

    elf.entry as usize
}

fn get_frame_buffer(bs: &BootServices) -> frame_buffer::FrameBuffer {
    let gop = bs.locate_protocol::<GraphicsOutput>().unwrap_success();
    let gop = unsafe { &mut *gop.get() };
    frame_buffer::FrameBuffer {
        frame_buffer: gop.frame_buffer().as_mut_ptr(),
        stride: gop.current_mode_info().stride() as u32,
        resolution: (
            gop.current_mode_info().resolution().0 as u32,
            gop.current_mode_info().resolution().1 as u32,
        ),
        format: match gop.current_mode_info().pixel_format() {
            PixelFormat::Rgb => frame_buffer::PixelFormat::Rgb,
            PixelFormat::Bgr => frame_buffer::PixelFormat::Bgr,
            f => panic!("Unsupported pixel format: {:?}", f),
        },
    }
}

fn exit_boot_services(
    image: Handle,
    st: SystemTable<Boot>,
) -> (SystemTable<Runtime>, memory_map::MemoryMap) {
    let enough_mmap_size =
        st.boot_services().memory_map_size() + 8 * mem::size_of::<MemoryDescriptor>();
    let mmap_buf = vec![0; enough_mmap_size].leak();
    let mut descriptors = Vec::with_capacity(enough_mmap_size);
    let (st, raw_descriptors) = st
        .exit_boot_services(image, mmap_buf)
        .expect_success("Failed to exit boot services");

    // uefi::MemoryDescriptor -> memory_map::Descriptor
    for d in raw_descriptors {
        if is_available_after_exit_boot_services(d.ty) {
            descriptors.push(memory_map::Descriptor {
                phys_start: d.phys_start,
                phys_end: d.phys_start + d.page_count * UEFI_PAGE_SIZE as u64,
            });
        }
    }
    let memory_map = {
        let (ptr, len, _) = descriptors.into_raw_parts();
        memory_map::MemoryMap {
            descriptors: ptr as *const memory_map::Descriptor,
            descriptors_len: len as u64,
        }
    };
    (st, memory_map)
}

fn is_available_after_exit_boot_services(ty: MemoryType) -> bool {
    matches!(
        ty,
        MemoryType::CONVENTIONAL | MemoryType::BOOT_SERVICES_CODE | MemoryType::BOOT_SERVICES_DATA
    )
}
