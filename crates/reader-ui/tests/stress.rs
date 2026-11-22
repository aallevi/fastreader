//! Random books drawn on every screen, theme and mode, with random taps at
//! random places: nothing may panic and every redraw stays on the screen.

use reader_core::{App, Book, Key, Settings};
use reader_ui::themes::THEMES;
use reader_ui::{Canvas, Ui, CATALOG};

const PIECES: &[&str] = &[
    "word", "Ünïcødé", "(aside", "here)", "«quote»", "<<x>>", "{pause}", "{slow}", "{/slow}", "{verse}", "line", "{/verse}",
    "{image:0}", "{image:3}", "**bold**", "*it*", "\n# Title words", "\n", "\n\n", ".", ",", "contemporaneamente", "tomorrow/yesterday",
    "😀", "a)", "(", ")", "«", "»",
];

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
fn random_books_draw_everywhere() {
    let mut r = Rng(7);
    let keys = [Key::Up, Key::Down, Key::Left, Key::Right, Key::Select, Key::Menu];
    for round in 0..40 {
        let n = 1 + (r.next() % 80) as usize;
        let text = (0..n).map(|_| PIECES[(r.next() % PIECES.len() as u64) as usize]).collect::<Vec<_>>().join(" ");
        // Half the books carry a small picture section.
        let mut file = text.into_bytes();
        if round % 2 == 0 {
            file.extend_from_slice(reader_core::book::IMAGES_MAGIC);
            file.extend_from_slice(&1u16.to_le_bytes());
            file.extend_from_slice(&[7, 0, 5, 0]);
            file.extend(core::iter::repeat_n(0x55, 7 * 5 * 2));
        }
        let theme = (round % THEMES.len()) as u8;
        let s = Settings { theme, mode: (round % 4) as u8, size: (round % 3) as u8, position: (round % 2) as u8, parens: round % 3 != 0, ..Settings::default() };
        let mut a = App::new(Book::from_bytes(&file, "t"), CATALOG, s);
        let mut cv = Canvas::new(320, 240);
        let mut ui = Ui::new(320, 240);
        let mut now = 0;
        for _ in 0..50 {
            now += r.next() % 300;
            if r.next().is_multiple_of(2) {
                let (x, y) = ((r.next() % 320) as i32, (r.next() % 240) as i32);
                if let Some(hit) = ui.hit(&a, x, y) {
                    a.touch_down(Some(hit));
                    a.tap(hit, now);
                }
            } else {
                a.handle(keys[(r.next() % keys.len() as u64) as usize], now);
            }
            a.tick(now);
            while a.take_effect().is_some() {}
            for d in ui.paint(&mut a, now, &mut cv) {
                assert!(d.x >= 0 && d.y >= 0 && d.right() <= 320 && d.bottom() <= 240, "redraw off screen: {d:?}");
            }
        }
    }
}
