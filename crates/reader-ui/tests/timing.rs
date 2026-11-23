//! Rough per-word paint cost by theme and word position (run with --ignored --nocapture).
use reader_core::{App, Book, Settings};
use reader_ui::{Canvas, Ui, CATALOG};

#[test]
#[ignore]
fn word_paint_cost_by_theme() {
    let text = include_str!("../../../books/sample.md").repeat(20);
    for theme in 0..reader_ui::themes::THEMES.len() as u8 {
        for position in [0u8, 1] {
            let mut a = App::new(Book::new("t", &text), CATALOG, Settings { theme, position, ..Settings::default() });
            let mut cv = Canvas::new(320, 240);
            let mut ui = Ui::new(320, 240);
            ui.paint(&mut a, 0, &mut cv);
            a.reader.play(0);
            let t0 = std::time::Instant::now();
            let mut px = 0u64;
            for k in 0..400 {
                a.reader.seek(100 + k, 0);
                for d in ui.paint(&mut a, 0, &mut cv) {
                    px += (d.w * d.h) as u64;
                }
            }
            println!("{:12} position {position}: {:5.0} us/word, {:6} px pushed/word", reader_ui::themes::THEMES[theme as usize].name, t0.elapsed().as_micros() as f64 / 400.0, px / 400);
        }
    }
}
