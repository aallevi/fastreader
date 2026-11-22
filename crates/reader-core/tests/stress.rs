//! Random text through the whole reader: nothing may panic, and every
//! position the reader reports must stay inside the book.

use reader_core::app::{App, Catalog, Hit};
use reader_core::input::Key;
use reader_core::{Book, Settings};

const CAT: Catalog = Catalog { themes: &["A", "B"], fonts: &["x"] };
const PIECES: &[&str] = &[
    "word", "Ünïcødé", "(", ")", "«", "»", "<<", ">>", "<", ">", "{pause}", "{pause:9}", "{slow}", "{/slow}", "{verse}", "{/verse}",
    "{image:0}", "{image:x}", "{image:99999999999}", "**", "*", "***", "# ", "\n", "\n\n", " ", ".", ",", "…", "—", "a)", "1)",
    "antidisestablishmentarianism", "tomorrow/yesterday", "\t", "#", "{", "}", "😀", "\u{FFFC}", "<!-- title: x -->",
];

/// Small deterministic generator (xorshift), so failures repeat.
struct Rng(u64);
impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }
}

#[test]
fn random_books_never_break_the_reader() {
    let mut r = Rng(0x9E37_79B9_7F4A_7C15);
    let keys = [Key::Up, Key::Down, Key::Left, Key::Right, Key::Select, Key::Menu];
    for round in 0..300 {
        let n = (r.next() % 120) as usize;
        let sep = if round % 3 == 0 { "" } else { " " };
        let text: String = (0..n).map(|_| PIECES[(r.next() % PIECES.len() as u64) as usize]).collect::<Vec<_>>().join(sep);
        let book = Book::new("t", &text);
        let len = book.len();
        for i in 0..len {
            let _ = book.text_of(book.word(i).unwrap());
            let _ = reader_core::book::bracket_pair(&book, i);
            assert!(book.phrase(i, 3).end <= len && book.group(i, 3).end <= len);
        }
        let mut html = String::new();
        book.write_html(&mut html).unwrap();
        for mode in 0..4u8 {
            let mut a = App::new(Book::new("t", &text), CAT, Settings { mode, ..Settings::default() });
            let mut now = 0;
            for _ in 0..60 {
                now += r.next() % 400;
                match r.next() % 5 {
                    0 => {
                        a.handle_held(keys[(r.next() % 4) as usize], now);
                    }
                    1 => {
                        a.tap(Hit::Bookmark, now);
                    }
                    2 => {
                        a.tap(Hit::Group((r.next() % 4) as u8), now);
                    }
                    _ => {
                        a.handle(keys[(r.next() % keys.len() as u64) as usize], now);
                    }
                }
                a.release(now);
                a.tick(now);
                while a.take_effect().is_some() {}
                assert!(len == 0 || a.reader.pos() < len, "pos {} of {len} in {text:?}", a.reader.pos());
                assert!(a.reader.shown().end <= len);
            }
        }
    }
}

#[test]
fn image_section_garbage_is_harmless() {
    let mut r = Rng(42);
    for _ in 0..200 {
        let mut b = b"Some text {image:0} {image:1}".to_vec();
        b.extend_from_slice(reader_core::book::IMAGES_MAGIC);
        let n = (r.next() % 40) as usize;
        b.extend((0..n).map(|_| r.next() as u8));
        let book = Book::from_bytes(&b, "t");
        for n in 0..book.images.len() {
            let img = book.images.get(n).unwrap();
            assert_eq!(img.px.len(), img.w as usize * img.h as usize);
        }
    }
}
