pub fn hlt() {
    unsafe { asm!("hlt") };
}

pub fn io_in(addr: u16) -> u32 {
    let ret: u32;
    unsafe { asm!("in eax, dx", out("eax") ret, in("dx") addr) };
    ret
}

pub fn io_out(addr: u16, data: u32) {
    unsafe { asm!("out dx, eax", in("dx") addr, in("eax") data) };
}
