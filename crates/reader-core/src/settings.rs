//! User settings, their limits, and their tiny on-flash format.
//!
//! [`Settings::set`] is the single place a setting changes by name, used by
//! the web settings page; it validates and clamps, so no client can put the
//! device in a bad state.

use alloc::string::String;

/// How far ◀◀ / ▶▶ jump.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Skip {
    Sentence,
    Words(u8),
    Paragraph,
}

pub const SKIPS: [Skip; 6] = [Skip::Sentence, Skip::Words(5), Skip::Words(10), Skip::Words(25), Skip::Words(50), Skip::Paragraph];
pub const SKIP_NAMES: [&str; 6] = ["1 sentence", "5 words", "10 words", "25 words", "50 words", "1 paragraph"];

/// Pause lengths, in tenths of a word's time (15 = 1.5×).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Pacing {
    pub comma: u8,
    pub sentence: u8,
    pub paragraph: u8,
    /// Chapter titles stay this many times longer.
    pub title: u8,
    /// Long words get a little extra time.
    pub long_words: bool,
    /// Ease in over the first words after pressing play.
    pub ramp: bool,
}

impl Default for Pacing {
    fn default() -> Self {
        Pacing { comma: 15, sentence: 22, paragraph: 30, title: 20, long_words: true, ramp: true }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Settings {
    pub wpm: u16,
    pub theme: u8,
    pub font: u8,
    /// 0 small, 1 normal, 2 large.
    pub size: u8,
    /// Words per flash, 1–3.
    pub chunk: u8,
    /// Backlight level 1–5.
    pub brightness: u8,
    /// Screen turned 180°.
    pub flipped: bool,
    /// Last view: speed reading or pages.
    pub pages: bool,
    /// Index into [`SKIPS`].
    pub skip: u8,
    /// Show the sentence under the word when paused.
    pub context: bool,
    /// Which library book is open.
    pub book: u8,
    pub pacing: Pacing,
    /// Word index in the current book.
    pub pos: u32,
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            wpm: 475,
            theme: 0,
            font: 0,
            size: 1,
            chunk: 1,
            brightness: 4,
            flipped: false,
            pages: false,
            skip: 0,
            context: true,
            book: 0,
            pacing: Pacing::default(),
            pos: 0,
        }
    }
}

const MAGIC: [u8; 2] = *b"FR";
const VERSION: u8 = 5;
pub const SETTINGS_LEN: usize = 26; // magic 2 + version + wpm 2 + pos 4 + 16 fields + checksum

/// Every setting the web page can change: (name, min, max).
pub const FIELDS: [(&str, u16, u16); 17] = [
    ("wpm", 100, 1000),
    ("theme", 0, 255),
    ("font", 0, 255),
    ("size", 0, 2),
    ("chunk", 1, 3),
    ("brightness", 1, 5),
    ("flipped", 0, 1),
    ("pages", 0, 1),
    ("skip", 0, 5),
    ("context", 0, 1),
    ("comma", 10, 40),
    ("sentence", 10, 50),
    ("paragraph", 10, 60),
    ("title", 10, 40),
    ("long_words", 0, 1),
    ("ramp", 0, 1),
    ("book", 0, 255),
];

impl Settings {
    pub fn skip(&self) -> Skip {
        SKIPS[self.skip.min(SKIPS.len() as u8 - 1) as usize]
    }

    pub fn get(&self, name: &str) -> Option<u16> {
        let p = &self.pacing;
        Some(match name {
            "wpm" => self.wpm,
            "theme" => self.theme as u16,
            "font" => self.font as u16,
            "size" => self.size as u16,
            "chunk" => self.chunk as u16,
            "brightness" => self.brightness as u16,
            "flipped" => self.flipped as u16,
            "pages" => self.pages as u16,
            "skip" => self.skip as u16,
            "context" => self.context as u16,
            "comma" => p.comma as u16,
            "sentence" => p.sentence as u16,
            "paragraph" => p.paragraph as u16,
            "title" => p.title as u16,
            "long_words" => p.long_words as u16,
            "ramp" => p.ramp as u16,
            "book" => self.book as u16,
            _ => return None,
        })
    }

    /// Sets a field by name, clamped to its range. Unknown names are refused.
    pub fn set(&mut self, name: &str, value: u16) -> Result<(), String> {
        let (_, lo, hi) = FIELDS.iter().find(|(n, _, _)| *n == name).ok_or_else(|| alloc::format!("unknown setting {name}"))?;
        let v = value.clamp(*lo, *hi);
        let b = v as u8;
        let p = &mut self.pacing;
        match name {
            "wpm" => self.wpm = v,
            "theme" => self.theme = b,
            "font" => self.font = b,
            "size" => self.size = b,
            "chunk" => self.chunk = b,
            "brightness" => self.brightness = b,
            "flipped" => self.flipped = v != 0,
            "pages" => self.pages = v != 0,
            "skip" => self.skip = b,
            "context" => self.context = v != 0,
            "comma" => p.comma = b,
            "sentence" => p.sentence = b,
            "paragraph" => p.paragraph = b,
            "title" => p.title = b,
            "long_words" => p.long_words = v != 0,
            "ramp" => p.ramp = v != 0,
            "book" => self.book = b,
            _ => unreachable!("FIELDS and set() list the same names"),
        }
        Ok(())
    }

    pub fn to_bytes(&self) -> [u8; SETTINGS_LEN] {
        let mut b = [0u8; SETTINGS_LEN];
        b[0..2].copy_from_slice(&MAGIC);
        b[2] = VERSION;
        b[3..5].copy_from_slice(&self.wpm.to_le_bytes());
        b[5..9].copy_from_slice(&self.pos.to_le_bytes());
        for (i, (name, _, _)) in FIELDS.iter().enumerate().skip(1) {
            b[8 + i] = self.get(name).unwrap() as u8;
        }
        b[SETTINGS_LEN - 1] = checksum(&b[..SETTINGS_LEN - 1]);
        b
    }

    /// Reads a record; every value goes through [`Settings::set`], so a bad
    /// byte is clamped and can never produce an unusable device.
    pub fn from_bytes(b: &[u8]) -> Option<Self> {
        if b.len() < SETTINGS_LEN || b[0..2] != MAGIC || b[2] != VERSION || b[SETTINGS_LEN - 1] != checksum(&b[..SETTINGS_LEN - 1]) {
            return None;
        }
        let mut s = Settings::default();
        s.set("wpm", u16::from_le_bytes([b[3], b[4]])).ok()?;
        s.pos = u32::from_le_bytes([b[5], b[6], b[7], b[8]]);
        for (i, (name, _, _)) in FIELDS.iter().enumerate().skip(1) {
            s.set(name, b[8 + i] as u16).ok()?;
        }
        Some(s)
    }
}

fn checksum(b: &[u8]) -> u8 {
    b.iter().fold(0x5Au8, |acc, x| acc.rotate_left(1) ^ x)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip() {
        let mut s = Settings { wpm: 525, theme: 2, font: 3, size: 2, chunk: 3, brightness: 1, flipped: true, pages: true, pos: 123_456, ..Settings::default() };
        s.set("skip", 4).unwrap();
        s.set("comma", 33).unwrap();
        s.set("ramp", 0).unwrap();
        assert_eq!(Settings::from_bytes(&s.to_bytes()), Some(s));
    }

    #[test]
    fn rejects_corruption() {
        let mut b = Settings::default().to_bytes();
        b[6] ^= 1;
        assert_eq!(Settings::from_bytes(&b), None);
        assert_eq!(Settings::from_bytes(&[0xFF; SETTINGS_LEN]), None, "erased flash");
        assert_eq!(Settings::from_bytes(&[]), None);
    }

    #[test]
    fn set_clamps_and_refuses_unknown() {
        let mut s = Settings::default();
        s.set("wpm", 5000).unwrap();
        assert_eq!(s.wpm, 1000);
        s.set("chunk", 0).unwrap();
        assert_eq!(s.chunk, 1);
        assert!(s.set("rm -rf", 1).is_err());
        for (name, lo, hi) in FIELDS {
            s.set(name, hi).unwrap();
            assert_eq!(s.get(name), Some(hi), "{name}");
            s.set(name, lo).unwrap();
            assert_eq!(s.get(name), Some(lo), "{name}");
        }
    }
}
