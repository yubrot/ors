use log::error;
use ors_common::asm;

#[panic_handler]
fn panic(info: &core::panic::PanicInfo) -> ! {
    error!("{}", info);

    loop {
        asm::hlt()
    }
}
