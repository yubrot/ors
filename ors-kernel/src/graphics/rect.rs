use derive_new::new;

#[derive(PartialEq, Eq, Debug, Clone, Copy, Hash, new)]
pub struct Rect {
    pub x: i32,
    pub y: i32,
    pub w: u32,
    pub h: u32,
}

impl Rect {
    pub fn intersect(self, other: Self) -> Option<Self> {
        let lx = self.x.max(other.x);
        let ly = self.y.max(other.y);
        let rx = (self.x + self.w as i32).min(other.x + other.w as i32);
        let ry = (self.y + self.h as i32).min(other.y + other.h as i32);
        if rx < lx || ry < ly {
            return None;
        }
        Some(Self {
            x: lx,
            y: ly,
            w: (rx - lx) as u32,
            h: (ry - ly) as u32,
        })
    }

    pub fn contains(self, x: i32, y: i32) -> bool {
        self.x <= x && x < self.x + self.w as i32 && self.y <= y && y < self.y + self.h as i32
    }

    pub fn offset(self, x: i32, y: i32) -> Self {
        Self::new(self.x + x, self.y + y, self.w, self.h)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use log::trace;

    #[test_case]
    fn test_rect() {
        trace!("TESTING graphics::rect");
        assert!(Rect::new(0, 0, 100, 100).contains(50, 50));
        assert!(!Rect::new(0, 0, 100, 100).contains(-5, 10));
        assert_eq!(
            Rect::new(0, 0, 100, 100).intersect(Rect::new(15, 10, 120, 60)),
            Some(Rect::new(15, 10, 85, 60))
        );
        assert_eq!(
            Rect::new(30, 40, 60, 60).intersect(Rect::new(10, 10, 80, 20)),
            None
        );
    }
}
