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
/// How the text is presented.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mode {
    /// One word (or 2–3) at a time, centred.
    Word,
    /// Short meaningful groups, starting at a fixed point on the left.
    Phrase,
    /// The text glides past as one line.
    Ticker,
    /// Normal pages; ▶ moves a phrase highlight and turns pages.
    Page,
}

pub const MODES: [Mode; 4] = [Mode::Word, Mode::Phrase, Mode::Ticker, Mode::Page];
pub const MODE_NAMES: [&str; 4] = ["Word by word", "Phrases", "Ticker", "Page"];
pub const SPACING_NAMES: [&str; 3] = ["Tight", "Normal", "Airy"];
pub const PARAGRAPH_NAMES: [&str; 2] = ["Spaced", "Indented"];
pub const ALIGN_NAMES: [&str; 2] = ["Ragged right", "Justified"];
pub const POSITION_NAMES: [&str; 2] = ["Centred", "Recognition point"];
/// "Welcome back" card: after how many minutes away it shows (0 = never).
pub const WELCOME_MIN: [u16; 5] = [0, 10, 20, 60, 180];
pub const WELCOME_NAMES: [&str; 5] = ["Off", "After 10 minutes", "After 20 minutes", "After 1 hour", "After 3 hours"];
pub const BRACKET_NAMES: [&str; 2] = ["In the text", "As side marks"];
pub const PACE_NAMES: [&str; 2] = ["Base rate", "Real pace (pauses included)"];

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
    /// How long a picture stays: an index into `PICTURE_MS` (last = until ▶).
    pub picture: u8,
    /// Tables: false = the table below the word the whole time; true = its
    /// grid flashes before each cell (a beat more for it).
    pub table_flash: bool,
}

/// Sleep timer: after how many minutes with nothing happening (no touch,
/// no key, not reading) the reader goes to sleep by itself (0 = never).
pub const SLEEP_MIN: [u16; 6] = [0, 2, 5, 10, 20, 30];
pub const SLEEP_NAMES: [&str; 6] = ["Never", "After 2 minutes", "After 5 minutes", "After 10 minutes", "After 20 minutes", "After 30 minutes"];

pub const TABLE_NAMES: [&str; 2] = ["Below the word", "Flash before each cell"];

/// Picture display times, in ms; 0 = reading pauses on the picture until ▶.
pub const PICTURE_MS: [u64; 6] = [2_000, 3_000, 4_000, 8_000, 15_000, 0];
pub const PICTURE_NAMES: [&str; 6] = ["2 seconds", "3 seconds", "4 seconds", "8 seconds", "15 seconds", "Until I press play"];

impl Default for Pacing {
    fn default() -> Self {
        Pacing { comma: 15, sentence: 22, paragraph: 30, title: 20, long_words: true, ramp: true, picture: 2, table_flash: false }
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
    /// Index into [`MODES`].
    pub mode: u8,
    /// Words per group in Phrase and Page modes (1–3), as on the original e-reader.
    pub phrase: u8,
    /// Page view: 0 tight, 1 normal, 2 airy line spacing.
    pub spacing: u8,
    /// Page view: indent paragraphs like a printed book instead of spacing them.
    pub indent: bool,
    /// Page view: justified lines.
    pub justify: bool,
    /// Page view: highlight the current phrase even while paused.
    pub highlight: bool,
    /// Web view over Wi-Fi (off by default: the radio costs battery).
    pub wifi: bool,
    /// The wpm number means the real average pace, pauses included.
    pub true_pace: bool,
    /// Word and Phrase modes: show ( … ) as marks at the screen's sides.
    pub brackets: bool,
    /// Where words sit: 0 centred, 1 on a fixed recognition point.
    pub position: u8,
    /// Thin grey lines above and below the word, with ticks at the centre letter.
    pub guides: bool,
    /// The "welcome back" card after a break: index into WELCOME_MIN.
    pub welcome: u8,
    /// The four menu buttons each in their own colour (white icons).
    pub menu_colours: bool,
    /// The sleep timer: index into SLEEP_MIN.
    pub sleep_after: u8,
    /// With the recognition point: split words too long to fit in two
    /// flashes ("determi-" "nazione"), or keep them whole (shifted to fit).
    pub split_long: bool,
    /// Show a picture's description (when paused on it).
    pub captions: bool,
    /// The progress bar at the bottom of the reading screen.
    pub progress_bar: bool,
    /// Windows 98: the progress blocks in classic navy blue instead of grey.
    pub blue_blocks: bool,
    /// The book numbers when paused: %, words, time left, chapter.
    pub book_info: bool,
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
            // The owner's own choices (read from the reader, 2026-11-22).
            wpm: 500,
            theme: 3, // Windows 98
            font: 0,
            size: 1,
            chunk: 1,
            brightness: 1,
            flipped: false,
            mode: 0,
            phrase: 3,
            spacing: 1,
            indent: true,
            justify: true,
            highlight: true,
            wifi: false,
            true_pace: false,
            brackets: true,
            position: 1,
            split_long: false,
            guides: true,
            welcome: 4,
            menu_colours: false,
            sleep_after: 3,
            captions: false,
            progress_bar: true,
            blue_blocks: false,
            book_info: true,
            skip: 0,
            context: true,
            book: 0,
            pacing: Pacing::default(),
            pos: 0,
        }
    }
}

/// The settings record: "FRS2", length (u16), then `name=value;` for every
/// field and `pos=…`, then a checksum. Stored by name, so updates that add or
/// reorder settings keep everything the reader already has (new ones start
/// at their default) instead of starting over.
const MAGIC2: [u8; 4] = *b"FRS2";
pub const SETTINGS_LEN: usize = 512;
/// The old fixed layout (version 15), read once to carry settings over.
const OLD_MAGIC: [u8; 2] = *b"FR";
const OLD_VERSION: u8 = 15;
const OLD_FIELDS: [&str; 27] = [
    "wpm", "theme", "font", "size", "chunk", "brightness", "flipped", "mode", "skip", "context", "comma", "sentence", "paragraph", "title",
    "long_words", "ramp", "picture", "book", "phrase", "spacing", "indent", "justify", "highlight", "wifi", "true_pace", "brackets", "position",
];

/// Every setting the web page can change: (name, min, max).
pub const FIELDS: [(&str, u16, u16); 37] = [
    ("wpm", 100, 1000),
    ("theme", 0, 255),
    ("font", 0, 255),
    ("size", 0, 2),
    ("chunk", 1, 3),
    ("brightness", 1, 5),
    ("flipped", 0, 1),
    ("mode", 0, 3),
    ("skip", 0, 5),
    ("context", 0, 1),
    ("comma", 10, 40),
    ("sentence", 10, 50),
    ("paragraph", 10, 60),
    ("title", 10, 40),
    ("long_words", 0, 1),
    ("ramp", 0, 1),
    ("pictures", 0, 5),
    ("tables", 0, 1),
    ("book", 0, 255),
    ("phrase", 1, 3),
    ("spacing", 0, 2),
    ("indent", 0, 1),
    ("justify", 0, 1),
    ("highlight", 0, 1),
    ("wifi", 0, 1),
    ("true_pace", 0, 1),
    ("brackets", 0, 1),
    ("position", 0, 1),
    ("split_long", 0, 1),
    ("guides", 0, 1),
    ("welcome", 0, 4),
    ("menu_colours", 0, 1),
    ("sleep_after", 0, 5),
    ("captions", 0, 1),
    ("progress_bar", 0, 1),
    ("blue_blocks", 0, 1),
    ("book_info", 0, 1),
];

impl Settings {
    pub fn mode(&self) -> Mode {
        MODES[self.mode.min(MODES.len() as u8 - 1) as usize]
    }

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
            "mode" => self.mode as u16,
            "phrase" => self.phrase as u16,
            "spacing" => self.spacing as u16,
            "indent" => self.indent as u16,
            "justify" => self.justify as u16,
            "highlight" => self.highlight as u16,
            "wifi" => self.wifi as u16,
            "true_pace" => self.true_pace as u16,
            "brackets" => self.brackets as u16,
            "position" => self.position as u16,
            "split_long" => self.split_long as u16,
            "guides" => self.guides as u16,
            "welcome" => self.welcome as u16,
            "menu_colours" => self.menu_colours as u16,
            "sleep_after" => self.sleep_after as u16,
            "captions" => self.captions as u16,
            "progress_bar" => self.progress_bar as u16,
            "blue_blocks" => self.blue_blocks as u16,
            "book_info" => self.book_info as u16,
            "skip" => self.skip as u16,
            "context" => self.context as u16,
            "comma" => p.comma as u16,
            "sentence" => p.sentence as u16,
            "paragraph" => p.paragraph as u16,
            "title" => p.title as u16,
            "long_words" => p.long_words as u16,
            "ramp" => p.ramp as u16,
            "pictures" => p.picture as u16,
            "tables" => p.table_flash as u16,
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
            "mode" => self.mode = b,
            "phrase" => self.phrase = b,
            "spacing" => self.spacing = b,
            "indent" => self.indent = v != 0,
            "justify" => self.justify = v != 0,
            "highlight" => self.highlight = v != 0,
            "wifi" => self.wifi = v != 0,
            "true_pace" => self.true_pace = v != 0,
            "brackets" => self.brackets = v != 0,
            "position" => self.position = b,
            "split_long" => self.split_long = v != 0,
            "guides" => self.guides = v != 0,
            "welcome" => self.welcome = v as u8,
            "menu_colours" => self.menu_colours = v != 0,
            "sleep_after" => self.sleep_after = b,
            "captions" => self.captions = v != 0,
            "progress_bar" => self.progress_bar = v != 0,
            "blue_blocks" => self.blue_blocks = v != 0,
            "book_info" => self.book_info = v != 0,
            "skip" => self.skip = b,
            "context" => self.context = v != 0,
            "comma" => p.comma = b,
            "sentence" => p.sentence = b,
            "paragraph" => p.paragraph = b,
            "title" => p.title = b,
            "long_words" => p.long_words = v != 0,
            "ramp" => p.ramp = v != 0,
            "pictures" => p.picture = b,
            "tables" => p.table_flash = v != 0,
            "book" => self.book = b,
            _ => unreachable!("FIELDS and set() list the same names"),
        }
        Ok(())
    }

    pub fn to_bytes(&self) -> [u8; SETTINGS_LEN] {
        use core::fmt::Write;
        let mut text = String::new();
        for (name, _, _) in FIELDS.iter() {
            let _ = write!(text, "{name}={};", self.get(name).unwrap_or(0));
        }
        let _ = write!(text, "pos={};", self.pos);
        let mut b = [0xFFu8; SETTINGS_LEN];
        let n = text.len().min(SETTINGS_LEN - 7);
        b[0..4].copy_from_slice(&MAGIC2);
        b[4..6].copy_from_slice(&(n as u16).to_le_bytes());
        b[6..6 + n].copy_from_slice(&text.as_bytes()[..n]);
        b[6 + n] = checksum(&b[..6 + n]);
        b
    }

    /// Reads a record; every value goes through [`Settings::set`], so a bad
    /// value is clamped and can never produce an unusable device. Unknown
    /// names are skipped; missing ones keep their default.
    pub fn from_bytes(b: &[u8]) -> Option<Self> {
        if b.len() >= 7 && b[0..4] == MAGIC2 {
            let n = u16::from_le_bytes([b[4], b[5]]) as usize;
            if 7 + n > b.len() || b[6 + n] != checksum(&b[..6 + n]) {
                return None;
            }
            let text = core::str::from_utf8(&b[6..6 + n]).ok()?;
            let mut s = Settings::default();
            for pair in text.split(';').filter(|p| !p.is_empty()) {
                let (k, v) = pair.split_once('=')?;
                if k == "pos" {
                    s.pos = v.parse().ok()?;
                } else if let Ok(v) = v.parse::<u16>() {
                    let _ = s.set(k, v);
                }
            }
            return Some(s);
        }
        // The old fixed layout (one byte per field in a set order).
        let len = 9 + OLD_FIELDS.len();
        if b.len() >= len && b[0..2] == OLD_MAGIC && b[2] == OLD_VERSION && b[len - 1] == checksum(&b[..len - 1]) {
            let mut s = Settings::default();
            s.set("wpm", u16::from_le_bytes([b[3], b[4]])).ok()?;
            s.pos = u32::from_le_bytes([b[5], b[6], b[7], b[8]]);
            for (i, name) in OLD_FIELDS.iter().enumerate().skip(1) {
                let _ = s.set(name, b[8 + i] as u16);
            }
            return Some(s);
        }
        None
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
        let mut s = Settings { wpm: 525, theme: 2, font: 3, size: 2, chunk: 3, brightness: 1, flipped: true, mode: 3, pos: 123_456, ..Settings::default() };
        s.set("skip", 4).unwrap();
        s.set("comma", 33).unwrap();
        s.set("ramp", 0).unwrap();
        assert_eq!(Settings::from_bytes(&s.to_bytes()), Some(s));
    }

    #[test]
    fn the_old_layout_carries_over() {
        // Read from the reader on 2026-11-22 (version 15, fixed layout).
        let old: [u8; 36] = [
            0x46, 0x52, 0x0f, 0xf4, 0x01, 0x15, 0x97, 0x00, 0x00, 0x03, 0x00, 0x01, 0x01, 0x01, 0x00, 0x00, 0x00, 0x01, 0x0f, 0x16, 0x1e,
            0x14, 0x01, 0x01, 0x01, 0x07, 0x03, 0x01, 0x01, 0x01, 0x01, 0x00, 0x00, 0x01, 0x01, 0x86,
        ];
        let s = Settings::from_bytes(&old).expect("the reader's own record reads");
        assert_eq!((s.wpm, s.theme, s.brightness, s.book, s.position, s.pos), (500, 3, 1, 7, 1, 38_677));
        assert_eq!(Settings::from_bytes(&s.to_bytes()), Some(s), "and is kept in the new format");
    }

    #[test]
    fn new_settings_start_at_their_default() {
        // A record from before a setting existed: everything else is kept.
        let mut b = [0xFFu8; SETTINGS_LEN];
        let text = b"wpm=600;theme=2;pos=42;";
        b[0..4].copy_from_slice(b"FRS2");
        b[4..6].copy_from_slice(&(text.len() as u16).to_le_bytes());
        b[6..6 + text.len()].copy_from_slice(text);
        b[6 + text.len()] = checksum(&b[..6 + text.len()]);
        let s = Settings::from_bytes(&b).unwrap();
        assert_eq!((s.wpm, s.theme, s.pos), (600, 2, 42));
        assert_eq!(s.pacing.picture, Settings::default().pacing.picture);
    }

    #[test]
    fn rejects_corruption() {
        let mut b = Settings::default().to_bytes();
        b[8] ^= 1;
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
