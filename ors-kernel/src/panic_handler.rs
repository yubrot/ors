use log::error;
use x86_64::instructions as asm;

#[panic_handler]
fn panic(info: &core::panic::PanicInfo) -> ! {
    error!("{}", info);

    loop {
        asm::hlt()
    }
}
