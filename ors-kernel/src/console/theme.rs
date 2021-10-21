use super::ansi::ColorScheme;

#[derive(Debug)]
pub struct OneMonokai;

impl ColorScheme for OneMonokai {
    fn foreground(&self) -> (u8, u8, u8) {
        (0xab, 0xb2, 0xbf)
    }

    fn background(&self) -> (u8, u8, u8) {
        (0x28, 0x2c, 0x34)
    }

    fn black(&self) -> (u8, u8, u8) {
        (0x2d, 0x31, 0x39)
    }
    fn red(&self) -> (u8, u8, u8) {
        (0xe0, 0x6c, 0x75)
    }

    fn green(&self) -> (u8, u8, u8) {
        (0x98, 0xc3, 0x79)
    }

    fn yellow(&self) -> (u8, u8, u8) {
        (0xe5, 0xc0, 0x7b)
    }

    fn blue(&self) -> (u8, u8, u8) {
        (0x52, 0x8b, 0xff)
    }

    fn magenta(&self) -> (u8, u8, u8) {
        (0xc6, 0x78, 0xdd)
    }

    fn cyan(&self) -> (u8, u8, u8) {
        (0x56, 0xb2, 0xc2)
    }

    fn white(&self) -> (u8, u8, u8) {
        (0xd7, 0xda, 0xe0)
    }

    fn bright_black(&self) -> (u8, u8, u8) {
        (0x7f, 0x84, 0x8e)
    }

    fn bright_red(&self) -> (u8, u8, u8) {
        (0xf4, 0x47, 0x47)
    }

    fn bright_green(&self) -> (u8, u8, u8) {
        (0x98, 0xc3, 0x79)
    }

    fn bright_yellow(&self) -> (u8, u8, u8) {
        (0xe5, 0xc0, 0x7b)
    }

    fn bright_blue(&self) -> (u8, u8, u8) {
        (0x52, 0x8b, 0xff)
    }

    fn bright_magenta(&self) -> (u8, u8, u8) {
        (0x7e, 0x00, 0x97)
    }

    fn bright_cyan(&self) -> (u8, u8, u8) {
        (0x56, 0xb6, 0xc2)
    }

    fn bright_white(&self) -> (u8, u8, u8) {
        (0xd7, 0xda, 0xe0)
    }
}
