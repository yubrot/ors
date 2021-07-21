use log::error;
use ors_common::hlt;

#[panic_handler]
fn panic(info: &core::panic::PanicInfo) -> ! {
    error!("{}", info);

    loop {
        hlt!()
    }
}
