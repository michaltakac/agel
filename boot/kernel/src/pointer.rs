//! Bounded PS/2 packet decoding; no device access or language policy.
pub struct Pointer {
    bytes: [u8; 3],
    length: usize,
    pub x: i32,
    pub y: i32,
    down: bool,
    /// The last position the screen has, so the pointer stays on it.
    right: i32,
    bottom: i32,
}

impl Pointer {
    /// A pointer in the middle of a `width` by `height` screen.
    pub const fn new(width: i32, height: i32) -> Self {
        Self {
            bytes: [0; 3],
            length: 0,
            x: width / 2,
            y: height / 2,
            down: false,
            right: width - 1,
            bottom: height - 1,
        }
    }

    /// Whether the button is held, as of the last packet.
    pub fn down(&self) -> bool {
        self.down
    }

    /// An absolute position, `x` and `y` in 65536ths of the screen, and
    /// whether the button is held; answers whether this is a press edge.
    pub fn absolute(&mut self, x: u16, y: u16, down: bool) -> bool {
        // To the nearest pixel: the host's own pointer sits on one.
        self.x = ((i64::from(x) * i64::from(self.right) + 32767) / 65535) as i32;
        self.y = ((i64::from(y) * i64::from(self.bottom) + 32767) / 65535) as i32;
        let pressed = down && !self.down;
        self.down = down;
        pressed
    }

    pub fn feed(&mut self, byte: u8) -> Option<bool> {
        if self.length == 0 && byte & 8 == 0 {
            return None;
        }
        self.bytes[self.length] = byte;
        self.length += 1;
        if self.length != 3 {
            return None;
        }
        self.length = 0;
        let [flags, x, y] = self.bytes;
        if flags & 0xc0 != 0 {
            return None;
        }
        let dx = i32::from(x) - if flags & 0x10 != 0 { 256 } else { 0 };
        let dy = i32::from(y) - if flags & 0x20 != 0 { 256 } else { 0 };
        self.x = (self.x + dx).clamp(0, self.right);
        self.y = (self.y - dy).clamp(0, self.bottom);
        let down = flags & 1 != 0;
        let pressed = down && !self.down;
        self.down = down;
        Some(pressed)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn packets_sign_edges_overflow_and_click_debounce() {
        let mut p = Pointer::new(1024, 768);
        assert_eq!(p.feed(0), None);
        for byte in [0x38, 255] {
            assert_eq!(p.feed(byte), None);
        }
        assert_eq!(p.feed(255), Some(false));
        assert_eq!((p.x, p.y), (511, 385));
        for (flags, expected) in [(9, true), (9, false), (8, false), (9, true)] {
            p.feed(flags);
            p.feed(0);
            let pressed = p.feed(0).unwrap();
            assert_eq!(pressed, expected);
        }
        p.feed(0xc8);
        p.feed(255);
        assert_eq!(p.feed(255), None);
        assert_eq!((p.x, p.y), (511, 385));
    }

    #[test]
    fn absolute_positions_map_to_the_nearest_pixel_with_press_edges() {
        let mut p = Pointer::new(1920, 1080);
        assert!(!p.absolute(0, 0, false));
        assert_eq!((p.x, p.y), (0, 0));
        assert!(p.absolute(65535, 65535, true));
        assert_eq!((p.x, p.y), (1919, 1079));
        // Held, not pressed again; released; pressed again.
        assert!(!p.absolute(32768, 32768, true));
        assert_eq!((p.x, p.y), (960, 540));
        assert!(!p.absolute(32768, 32768, false));
        assert!(p.absolute(25954, 8190, true));
        assert_eq!((p.x, p.y), (760, 135));
    }
}
