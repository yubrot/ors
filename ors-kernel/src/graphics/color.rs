#[derive(PartialEq, Eq, PartialOrd, Ord, Debug, Clone, Copy, Hash)]
pub struct Color {
    pub r: u8,
    pub g: u8,
    pub b: u8,
}

impl Color {
    pub const fn new(r: u8, g: u8, b: u8) -> Self {
        Self { r, g, b }
    }

    pub fn mix(self, other: Self, f: f32) -> Self {
        let r = self.r as f32 * (1.0 - f) + other.r as f32 * f;
        let g = self.g as f32 * (1.0 - f) + other.g as f32 * f;
        let b = self.b as f32 * (1.0 - f) + other.b as f32 * f;
        Self::new(r as u8, g as u8, b as u8)
    }

    pub const WHITE: Self = Self::new(222, 222, 222);
    pub const BLACK: Self = Self::new(33, 33, 33);
}
