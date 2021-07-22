pub fn hlt() {
    unsafe { asm!("hlt") };
}
