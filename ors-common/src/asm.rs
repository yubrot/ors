#[macro_export]
macro_rules! hlt {
    () => {
        unsafe {
            asm!("hlt");
        }
    };
}
