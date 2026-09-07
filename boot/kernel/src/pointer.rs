//! Bounded PS/2 packet decoding; no device access or language policy.
pub struct Pointer {
    bytes: [u8; 3],
    length: usize,
    pub x: i32,
    pub y: i32,
    down: bool,
}

impl Pointer {
    pub const fn new() -> Self {
        Self {
            bytes: [0; 3],
            length: 0,
            x: 512,
            y: 384,
            down: false,
        }
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
        self.x = (self.x + dx).clamp(0, 1023);
        self.y = (self.y - dy).clamp(0, 767);
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
        let mut p = Pointer::new();
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
}
