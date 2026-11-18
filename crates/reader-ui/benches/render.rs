//! `cargo bench -p reader-ui`. Desktop timings; the ESP32-S3 at 240 MHz is
//! roughly 20–40× slower, so a word update must stay well under 1 ms here
//! for 1000 wpm (60 ms per word) to be comfortable on the device.

use criterion::{criterion_group, criterion_main, Criterion};
use reader_core::{App, Book, Key, Settings};
use reader_ui::{Canvas, Ui, CATALOG};
use std::hint::black_box;

const TEXT: &str = include_str!("../../../books/sample.md");

fn benches(c: &mut Criterion) {
    let novel = TEXT.repeat(400);
    c.bench_function("tokenize a novel (≈70k words)", |b| b.iter(|| Book::new("n", black_box(&novel))));
    for theme in 0..CATALOG.themes.len() as u8 {
        let name = CATALOG.themes[theme as usize];
        let mut app = App::new(Book::new("n", &TEXT.repeat(60)), CATALOG, Settings { theme, ..Settings::default() });
        let mut cv = Canvas::new(320, 240);
        let mut ui = Ui::new(320, 240);
        app.handle(Key::Select, 0);
        app.tick(5_000);
        ui.paint(&mut app, 5_000, &mut cv);
        let mut pos = 50;
        c.bench_function(&format!("word update [{name}]"), |b| {
            b.iter(|| {
                pos = 50 + (pos + 1) % 400;
                app.reader.seek(pos, 5_000);
                black_box(ui.paint(&mut app, 5_000, &mut cv));
            })
        });
        c.bench_function(&format!("full repaint [{name}]"), |b| {
            b.iter(|| {
                ui.invalidate();
                black_box(ui.paint(&mut app, 5_000, &mut cv));
            })
        });
    }
    let mut app = App::new(Book::new("n", &novel), CATALOG, Settings { mode: 3, ..Settings::default() });
    let mut cv = Canvas::new(320, 240);
    c.bench_function("paginate a novel", |b| {
        b.iter(|| {
            let mut ui = Ui::new(320, 240);
            app.pages.clear();
            black_box(ui.paint(&mut app, 0, &mut cv));
        })
    });
}

criterion_group!(name = g; config = Criterion::default().sample_size(10).measurement_time(std::time::Duration::from_secs(2)); targets = benches);
criterion_main!(g);
