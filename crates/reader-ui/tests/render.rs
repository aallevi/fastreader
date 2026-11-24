//! Rendering checks: every theme/screen/size renders, redraws stay small,
//! feedback is always visible, and touch targets match what is drawn.

use reader_core::app::{menu_index, Battery, MenuItem, PickKind, MENU_LEN};
use reader_core::{App, Book, Hit, Key, Screen, Settings};
use reader_ui::layout::toolbar;
use reader_ui::themes::THEMES;
use reader_ui::{Canvas, Ui, CATALOG};

const TEXT: &str = include_str!("../../../books/sample.md");

fn app_with(text: &str, s: Settings) -> App {
    let mut a = App::new(Book::new("Sample", text), CATALOG, s);
    a.battery = Some(Battery { percent: 64, charging: false, millivolts: 3900 });
    a
}

fn app(theme: u8) -> App {
    app_with(TEXT, Settings { theme, ..Settings::default() })
}

#[test]
fn every_theme_screen_size_and_font_renders() {
    let mut screens = vec![Screen::Reading, Screen::Line { resume: false }, Screen::Page, Screen::Info];
    screens.extend((0..MENU_LEN).map(|sel| Screen::Menu { sel }));
    screens.push(Screen::Pick { kind: PickKind::Font, sel: 3, original: 1 });
    screens.push(Screen::Pick { kind: PickKind::Speed, sel: 11, original: 0 });
    for (w, h) in [(320, 240), (240, 320), (480, 320), (296, 160)] {
        for theme in 0..THEMES.len() as u8 {
            for font in 0..CATALOG.fonts.len() as u8 {
                for size in 0..3 {
                    let mode = (font + size) % 4; // cover every mode without a 4th loop
                    let mut a = app_with(TEXT, Settings { theme, font, size, mode, ..Settings::default() });
                    a.reader.seek(37, 0);
                    for &s in &screens {
                        a.screen = s;
                        let mut cv = Canvas::new(w, h);
                        let dmg = Ui::new(w, h).paint(&mut a, 0, &mut cv);
                        assert_eq!(dmg.len(), 1, "first paint is one full repaint");
                    }
                }
            }
        }
    }
}

#[test]
fn word_change_repaints_only_the_word() {
    for theme in 0..THEMES.len() as u8 {
        // Novel-length text so the progress bar doesn't move on every word.
        let mut a = app_with(&TEXT.repeat(60), Settings { theme, ..Settings::default() });
        let (w, h) = (320, 240);
        let mut cv = Canvas::new(w, h);
        let mut ui = Ui::new(w, h);
        a.handle(Key::Select, 0);
        a.tick(5_000);
        ui.paint(&mut a, 5_000, &mut cv);
        a.reader.seek(40, 5_000);
        cv.writes = 0;
        let dmg = ui.paint(&mut a, 5_001, &mut cv);
        let area: i32 = dmg.iter().map(|r| r.w * r.h).sum();
        let pct = area * 100 / (w * h) as i32;
        assert!(pct <= 40, "{}: pushed {pct}% of the screen ({dmg:?})", THEMES[theme as usize].name);
        assert!(cv.writes * 100 / (w * h) as u64 <= 45, "{}: wrote {} px", THEMES[theme as usize].name, cv.writes);
    }
}

#[test]
fn idle_frames_cost_nothing() {
    let mut a = app(0);
    let mut cv = Canvas::new(320, 240);
    let mut ui = Ui::new(320, 240);
    ui.paint(&mut a, 0, &mut cv);
    cv.writes = 0;
    for t in 1..20 {
        a.tick(t);
        assert!(ui.paint(&mut a, t, &mut cv).is_empty());
    }
    assert_eq!(cv.writes, 0);
}

#[test]
fn every_key_shows_feedback_in_the_title_bar_or_by_lighting_its_button() {
    for theme in 0..THEMES.len() as u8 {
        let mut a = app(theme);
        a.reader.seek(20, 0);
        let mut cv = Canvas::new(320, 240);
        let mut ui = Ui::new(320, 240);
        ui.paint(&mut a, 0, &mut cv);
        let l = ui.layout;
        for (i, key) in [Key::Up, Key::Down, Key::Left, Key::Right, Key::Select, Key::Select].into_iter().enumerate() {
            let now = 1_000 + i as u64 * 2_000;
            a.tick(now);
            ui.paint(&mut a, now, &mut cv);
            a.handle(key, now);
            let dmg = ui.paint(&mut a, now, &mut cv);
            assert!(dmg.contains(&l.toolbar), "{key:?}: its button didn't light");
            assert!(dmg.contains(&l.header) || dmg.contains(&l.screen), "{key:?}: no message");
        }
    }
}

#[test]
fn toolbar_buttons_hit_what_they_draw() {
    for theme in 0..THEMES.len() as u8 {
        let mut a = app(theme);
        for screen in [Screen::Reading, Screen::Page] {
            a.screen = screen;
            let ui = Ui::new(320, 240);
            for b in toolbar(&a, &ui.layout).0 {
                assert_eq!(ui.hit(&a, b.rect.cx() as i32, b.rect.cy() as i32), Some(b.hit));
            }
        }
    }
}

#[test]
fn tapping_a_menu_row_opens_it() {
    let mut a = app(0);
    let mut cv = Canvas::new(320, 240);
    let mut ui = Ui::new(320, 240);
    ui.paint(&mut a, 0, &mut cv);
    // The ⚙ Settings button in the title bar, through hit testing.
    let gear = reader_ui::layout::header_buttons(&ui.layout)[0];
    let hit = ui.hit(&a, gear.rect.cx() as i32, gear.rect.cy() as i32).unwrap();
    a.tap(hit, 0);
    ui.paint(&mut a, 0, &mut cv);
    // Settings > Reading > Speed, tapping each row.
    let g = reader_ui::layout::list_geom(&a, &ui.layout);
    let row = g.rows.iter().find(|r| r.kind == reader_ui::layout::RowKind::Item(menu_index(MenuItem::SubReading))).unwrap();
    let hit = ui.hit(&a, row.rect.cx() as i32, row.rect.cy() as i32).unwrap();
    a.tap(hit, 5);
    ui.paint(&mut a, 5, &mut cv);
    assert_eq!(reader_ui::layout::list_geom(&a, &ui.layout).title, "Reading", "the submenu's name on top");
    let g = reader_ui::layout::list_geom(&a, &ui.layout);
    let row = g.rows.iter().find(|r| r.kind == reader_ui::layout::RowKind::Item(menu_index(MenuItem::Speed))).unwrap();
    let hit = ui.hit(&a, row.rect.cx() as i32, row.rect.cy() as i32).unwrap();
    assert_eq!(hit, Hit::Row(menu_index(MenuItem::Speed)));
    a.tap(hit, 10);
    assert!(matches!(a.screen, Screen::Pick { kind: PickKind::Speed, .. }));
}

#[test]
fn pages_cover_the_whole_book_in_order() {
    for size in 0..3 {
        let mut a = app_with(&TEXT.repeat(5), Settings { mode: 3, size, ..Settings::default() });
        let mut cv = Canvas::new(320, 240);
        let mut ui = Ui::new(320, 240);
        let mut frames = 0;
        while !a.pages_done {
            ui.paint(&mut a, frames, &mut cv);
            frames += 1;
            assert!(frames < 1000, "pagination never finishes");
        }
        assert!(a.pages.len() > 3, "size {size}: {} pages", a.pages.len());
        assert_eq!(a.pages[0], 0);
        assert!(a.pages.windows(2).all(|w| w[0] < w[1]), "pages strictly increase");
        assert!((*a.pages.last().unwrap() as usize) < a.reader.book.len());
    }
}

#[test]
fn theme_change_repaints_everything() {
    let mut a = app(0);
    let mut cv = Canvas::new(320, 240);
    let mut ui = Ui::new(320, 240);
    ui.paint(&mut a, 0, &mut cv);
    a.settings.theme = 2;
    assert_eq!(ui.paint(&mut a, 1, &mut cv), vec![ui.layout.screen]);
}

#[test]
fn damage_stays_on_screen_during_a_long_read() {
    let mut a = app(1);
    let mut cv = Canvas::new(320, 240);
    let mut ui = Ui::new(320, 240);
    a.handle(Key::Select, 0);
    let screen = cv.bounds();
    for t in (0..60_000).step_by(23) {
        a.tick(t);
        for r in ui.paint(&mut a, t, &mut cv) {
            assert_eq!(r.intersect(&screen), r);
        }
    }
}

#[test]
fn every_typography_option_paginates_the_whole_book() {
    for spacing in 0..3 {
        for (indent, justify) in [(false, false), (true, false), (false, true), (true, true)] {
            let mut a = app_with(&TEXT.repeat(4), Settings { mode: 3, spacing, indent, justify, ..Settings::default() });
            let mut cv = Canvas::new(320, 240);
            let mut ui = Ui::new(320, 240);
            for t in 0..400 {
                ui.paint(&mut a, t, &mut cv);
                if a.pages_done {
                    break;
                }
            }
            assert!(a.pages_done);
            assert!(a.pages.windows(2).all(|w| w[0] < w[1]));
            // Every word is on exactly one page: page starts never skip text.
            assert_eq!(a.pages[0], 0);
        }
    }
}

#[test]
fn airy_spacing_needs_more_pages_than_tight() {
    let count = |spacing| {
        let mut a = app_with(&TEXT.repeat(4), Settings { mode: 3, spacing, ..Settings::default() });
        let mut cv = Canvas::new(320, 240);
        let mut ui = Ui::new(320, 240);
        for t in 0..400 {
            ui.paint(&mut a, t, &mut cv);
        }
        a.pages.len()
    };
    assert!(count(2) > count(0));
}

#[test]
fn ticker_steps_like_a_typewriter() {
    let mut a = app_with(&TEXT.repeat(20), Settings { mode: 2, ..Settings::default() });
    let mut cv = Canvas::new(320, 240);
    let mut ui = Ui::new(320, 240);
    a.handle(Key::Select, 0);
    ui.paint(&mut a, 0, &mut cv);
    // Let the "Reading" message and the button flash expire first.
    a.tick(1990);
    ui.paint(&mut a, 1990, &mut cv);
    let mut frames = 0;
    let start = a.reader.pos();
    for t in (2000..6000).step_by(33) {
        a.tick(t);
        let dmg = ui.paint(&mut a, t, &mut cv);
        if !dmg.is_empty() {
            frames += 1;
            let area: i32 = dmg.iter().map(|r| r.w * r.h).sum();
            assert!(area * 100 / (320 * 240) <= 45, "frame pushed {area} px: {dmg:?}");
        }
    }
    let words = a.reader.pos() - start;
    assert!(words >= 4, "it kept reading ({words} words)");
    assert!(frames <= words + 1, "one jump per word, still in between ({frames} frames for {words} words)");
}

#[test]
fn highlight_dot_is_tappable_in_page_mode() {
    let mut a = app_with(TEXT, Settings { mode: 3, ..Settings::default() });
    let ui = Ui::new(320, 240);
    let b = reader_ui::layout::highlight_button(&ui.layout);
    assert_eq!(ui.hit(&a, b.rect.cx() as i32, b.rect.cy() as i32), Some(Hit::Highlight));
    a.tap(Hit::Highlight, 0);
    assert!(!a.settings.highlight);
}

#[test]
fn page_highlight_moves_by_redrawing_only_a_few_lines() {
    let mut a = app_with(&TEXT.repeat(20), Settings { mode: 3, ..Settings::default() });
    let mut cv = Canvas::new(320, 240);
    let mut ui = Ui::new(320, 240);
    for t in 0..50 {
        ui.paint(&mut a, t, &mut cv);
    }
    a.handle(Key::Select, 100);
    ui.paint(&mut a, 100, &mut cv);
    a.tick(1_990);
    ui.paint(&mut a, 1_990, &mut cv);
    let mut checked = 0;
    let mut now = 2_000;
    while checked < 10 {
        let before = a.reader.pos();
        let page = a.page_index();
        now += 20;
        a.tick(now);
        let dmg = ui.paint(&mut a, now, &mut cv);
        if a.reader.pos() != before && a.page_index() == page {
            let area: i32 = dmg.iter().map(|r| r.w * r.h).sum();
            // Usually 2 lines (~20%); up to 3 when leaving a two-line title.
            assert!(area * 100 / (320 * 240) <= 40, "highlight step pushed {area} px: {dmg:?}");
            checked += 1;
        }
    }
}

#[test]
fn page_deep_in_a_long_book_shows_without_counting_first() {
    let mut a = app_with(&TEXT.repeat(400), Settings { mode: 3, ..Settings::default() });
    a.reader.seek(a.reader.book.len() * 4 / 5, 0);
    let mut cv = Canvas::new(320, 240);
    let mut ui = Ui::new(320, 240);
    ui.paint(&mut a, 0, &mut cv);
    assert!(!a.pages_done, "one frame only counts a few pages");
    assert_eq!(a.page_index(), None);
    let before = a.reader.pos();
    a.handle(Key::Right, 10);
    assert!(a.reader.pos() > before, "stepping works before the count is done");
}

#[test]
fn centred_words_are_centred() {
    let th = THEMES[0];
    let mut a = app_with(&TEXT.repeat(3), Settings { mode: 0, chunk: 1, position: 0, ..Settings::default() });
    let mut cv = Canvas::new(320, 240);
    let mut ui = Ui::new(320, 240);
    let l = ui.layout;
    for pos in [30usize, 45, 61] {
        a.reader.seek(pos, 0);
        a.handle(Key::Select, 0);
        ui.invalidate();
        ui.paint(&mut a, 0, &mut cv);
        a.handle(Key::Select, 0);
        // Below the corner's fold line.
        let ink = |x: i32| (l.word.y.max(l.stage.y + 34)..l.word.bottom()).any(|y| cv.pixel(x, y) != th.page);
        let left = (0..320).find(|&x| ink(x)).unwrap();
        let right = (0..320).rev().find(|&x| ink(x)).unwrap();
        assert!(((left + right) / 2 - 160).abs() <= 3, "word at {left}..{right}");
    }
}

#[test]
fn phrases_start_at_the_same_left_edge_every_flash() {
    let th = THEMES[0];
    let mut a = app_with(&TEXT.repeat(3), Settings { mode: 1, position: 1, ..Settings::default() });
    let mut cv = Canvas::new(320, 240);
    let mut ui = Ui::new(320, 240);
    let l = ui.layout;
    let mut lefts = Vec::new();
    for pos in [30usize, 45, 60, 75, 90, 120] {
        a.reader.seek(pos, 0);
        a.handle(Key::Select, 0); // play: hides the paused context line
        ui.invalidate();
        ui.paint(&mut a, 0, &mut cv);
        a.handle(Key::Select, 0);
        let left = (0..320).find(|&x| (l.word.y..l.word.bottom()).any(|y| cv.pixel(x, y) != th.page));
        lefts.push(left.expect("ink in the word band"));
    }
    let (min, max) = (lefts.iter().min().unwrap(), lefts.iter().max().unwrap());
    assert!(max - min <= 3, "left edges moved: {lefts:?}");
}

/// How well the recognition point holds on real text: in how many words the
/// letter can sit exactly on the fixation point (the rest are long words
/// that must shift to fit). Run with --ignored --nocapture to see figures.
#[test]
#[ignore]
fn recognition_point_on_real_books() {
    use reader_core::book::orp_index;
    let l = reader_ui::layout::Layout::new(320, 240);
    let p = l.stage;
    let fix = reader_ui::painter::fixation_x(p);
    let mut f = reader_ui::text::Fonts::new();
    for path in ["../../books/private/cancroregina.md", "../../books/private/differenza-e-ripetizione.md", "../../books/sample.md"] {
        let Ok(text) = std::fs::read_to_string(path) else { continue };
        let b = Book::parse(&text, "t");
        for (size, px) in [(0, 30.0f32), (1, 40.0), (2, 52.0)] {
            let face = reader_ui::text::family(0).face(reader_core::book::Style::PLAIN);
            let (mut off, mut n) = (0, 0);
            for i in (0..b.len()).step_by(7) {
                let w = b.text_of(b.word(i).unwrap());
                let total = f.measure(face, px, w);
                let x0 = fix - reader_ui::painter::recognition_x(&mut f, face, px, w);
                n += 1;
                if x0 < p.x as f32 + 8.0 || x0 + total > p.right() as f32 - 8.0 {
                    off += 1;
                }
                let _ = orp_index(w);
            }
            println!("{path} size {size}: {:.2}% of words must shift ({off} of {n})", off as f64 * 100.0 / n as f64);
        }
    }
}

#[test]
fn long_sentence_under_the_word_scrolls_with_arrows() {
    let long = "This sentence goes on and on with many words so that it cannot possibly fit on the two short lines under the word, and it keeps going with more and more words until the very end of it.";
    let mut a = app_with(&format!("Short one. {long} Next."), Settings { mode: 0, chunk: 1, ..Settings::default() });
    let mut cv = Canvas::new(320, 240);
    let mut ui = Ui::new(320, 240);
    let [up, down] = reader_ui::layout::context_arrows(&ui.layout);
    a.reader.seek(1, 0);
    ui.paint(&mut a, 0, &mut cv);
    assert_ne!(ui.hit(&a, down.cx() as i32, down.cy() as i32).map(|h| matches!(h, Hit::ContextScroll { .. })), Some(true), "a short sentence has no arrows");
    a.reader.seek(4, 0);
    ui.paint(&mut a, 10, &mut cv);
    let tap = |ui: &Ui, a: &App, r: reader_ui::Rect| ui.hit(a, r.cx() as i32, r.cy() as i32);
    assert_eq!(tap(&ui, &a, up), Some(Hit::ContextScroll { down: false, room: false }), "at the start: nothing above");
    assert_eq!(a.tap(Hit::ContextScroll { down: false, room: false }, 20).tone, reader_core::app::Tone::Limit);
    let h = tap(&ui, &a, down).unwrap();
    assert_eq!(h, Hit::ContextScroll { down: true, room: true });
    a.tap(h, 30);
    let dmg = ui.paint(&mut a, 40, &mut cv);
    assert_eq!(a.context_scroll, 1);
    assert!(dmg.contains(&ui.layout.context), "the sentence area redraws");
    a.reader.seek(20, 50);
    ui.paint(&mut a, 60, &mut cv);
    assert_eq!(a.context_scroll, 0, "a new word starts at its own line");
}

#[test]
fn both_halves_of_a_split_word_reach_the_screen() {
    // The halves share one position: the second must still be redrawn.
    let mut a = app_with("Era contemporaneamente qui.", Settings { mode: 0, chunk: 1, position: 1, split_long: true, size: 2, ..Settings::default() });
    let mut cv = Canvas::new(320, 240);
    let mut ui = Ui::new(320, 240);
    a.reader.seek(1, 0);
    ui.paint(&mut a, 0, &mut cv);
    a.reader.play(0);
    let mut redrawn_on_second_half = false;
    for t in (10..3000).step_by(10) {
        a.tick(t);
        let dmg = ui.paint(&mut a, t, &mut cv);
        if a.reader.split_part() == Some(true) && !dmg.is_empty() {
            redrawn_on_second_half = true;
            break;
        }
    }
    assert!(redrawn_on_second_half, "the second half was never sent to the screen");
}

#[test]
fn every_menu_entry_and_option_list_draws_in_every_theme() {
    use reader_core::app::{menu_item, MENU_LEN};
    for theme in 0..THEMES.len() as u8 {
        let mut a = app_with(TEXT, Settings { theme, ..Settings::default() });
        a.library = vec!["One".into(), "Two".into()];
        a.card = Some(reader_core::app::Card { total_mb: 15_000, free_mb: 14_000, formatted: true });
        let mut cv = Canvas::new(320, 240);
        let mut ui = Ui::new(320, 240);
        for sel in 0..MENU_LEN {
            let item = menu_item(sel);
            assert!(!item.label().is_empty());
            a.screen = Screen::Menu { sel };
            ui.invalidate();
            assert!(!ui.paint(&mut a, 0, &mut cv).is_empty(), "{item:?} menu draws");
            if let Some(kind) = item.pick() {
                for i in 0..a.pick_len(kind) {
                    assert!(!a.pick_label(kind, i).is_empty() || kind == PickKind::Bookmark, "{kind:?} row {i} has a label");
                }
                a.screen = Screen::Pick { kind, sel: 0, original: 0 };
                ui.invalidate();
                ui.paint(&mut a, 0, &mut cv);
            }
        }
    }
}

#[test]
fn guide_lines_give_way_to_pictures_and_come_back() {
    // A word, a picture, a word: the lines are hidden by the picture and back after.
    let mut file = b"Before the figure.\n\n{image:0}\n\nAfter it.".to_vec();
    file.extend_from_slice(reader_core::book::IMAGES_MAGIC);
    file.extend_from_slice(&1u16.to_le_bytes());
    file.extend_from_slice(&[100, 0, 60, 0]);
    file.extend(std::iter::repeat_n(0x00, 100 * 60 * 2)); // a black picture
    for theme in [0u8, 3] {
        let mut a = App::new(Book::from_bytes(&file, "t"), CATALOG, Settings { theme, guides: true, mode: 0, chunk: 1, ..Settings::default() });
        let mut cv = Canvas::new(320, 240);
        let mut ui = Ui::new(320, 240);
        let l = ui.layout;
        // Find a guide row: a row of the word area that is drawn across nearly
        // the whole width (the line), well away from the word itself.
        let line_rows = |cv: &Canvas| -> Vec<i32> {
            let page = cv.pixel(160, l.word.y + 2);
            (l.word.y..l.word.bottom())
                .filter(|&y| (20..300).filter(|&x| cv.pixel(x, y) != page).count() > 250)
                .collect()
        };
        a.reader.seek(1, 0);
        ui.paint(&mut a, 0, &mut cv);
        let before = line_rows(&cv);
        assert!(!before.is_empty(), "theme {theme}: guide lines drawn");
        a.reader.seek(3, 10); // the picture
        ui.paint(&mut a, 10, &mut cv);
        let pic_rows = line_rows(&cv);
        assert!(before.iter().all(|y| !pic_rows.contains(y)), "theme {theme}: the picture hides the lines");
        a.reader.seek(4, 20);
        ui.paint(&mut a, 20, &mut cv);
        assert_eq!(line_rows(&cv), before, "theme {theme}: the lines come back after the picture");
    }
}

#[test]
#[ignore]
fn guide_lines_pictures_sheet() {
    // cargo test -p reader-ui --test render guide_lines_pictures_sheet -- --ignored
    let mut file = b"Before the figure.\n\n{image:0}\n{caption} A small grey square. {/caption}\n\nAfter it.".to_vec();
    file.extend_from_slice(reader_core::book::IMAGES_MAGIC);
    file.extend_from_slice(&1u16.to_le_bytes());
    file.extend_from_slice(&[60, 0, 40, 0]);
    file.extend(std::iter::repeat_n(0x84, 60 * 40 * 2));
    let dir = std::env::var("SHEET_DIR").unwrap();
    for theme in [0u8, 3] {
        let mut a = App::new(Book::from_bytes(&file, "t"), CATALOG, Settings { theme, guides: true, mode: 0, chunk: 1, position: 1, ..Settings::default() });
        let mut cv = Canvas::new(320, 240);
        let mut ui = Ui::new(320, 240);
        a.settings.captions = true;
        for (k, (pos, t)) in [(1usize, 0u64), (3, 10), (4, 20)].into_iter().enumerate() {
            a.reader.seek(pos, t);
            ui.paint(&mut a, t, &mut cv);
            let mut ppm = b"P6 320 240 255\n".to_vec();
            for y in 0..240 {
                for x in 0..320 {
                    let c = cv.pixel(x, y);
                    ppm.extend_from_slice(&[(c >> 16) as u8, (c >> 8) as u8, c as u8]);
                }
            }
            std::fs::write(format!("{dir}/t{theme}-{k}.ppm"), ppm).unwrap();
        }
    }
}

#[test]
fn bookmark_corner_stays_on_top_of_pictures() {
    // A bookmarked word, then a bookmarked wide picture right after: the
    // corner doesn't change, so only the word area is redrawn, and the
    // picture must not paint over the folded corner (or wipe it on leaving).
    let mut file = b"Before the figure.\n\n{image:0}\n\nAfter it.".to_vec();
    file.extend_from_slice(reader_core::book::IMAGES_MAGIC);
    file.extend_from_slice(&1u16.to_le_bytes());
    file.extend_from_slice(&[0x40, 1, 0x82, 0]); // 320 x 130: reaches the corner
    file.extend(std::iter::repeat_n(0x00, 320 * 130 * 2));
    for theme in [0u8, 3] {
        let mut a = App::new(Book::from_bytes(&file, "t"), CATALOG, Settings { theme, mode: 0, chunk: 1, ..Settings::default() });
        let mut cv = Canvas::new(320, 240);
        let mut ui = Ui::new(320, 240);
        let corner = reader_ui::layout::bookmark_corner(&a, &ui.layout).rect;
        let px = |cv: &Canvas| -> Vec<u32> { (corner.y..corner.bottom()).flat_map(|y| (corner.x..corner.right()).map(move |x| (x, y))).map(|(x, y)| cv.pixel(x, y)).collect() };
        for (pos, t) in [(2usize, 0u64), (3, 10)] {
            a.reader.seek(pos, t);
            a.tap(Hit::Bookmark, t);
        }
        let mut t = 5_000;
        for pos in [2usize, 3, 2] {
            a.reader.seek(pos, t);
            a.tick(t);
            ui.paint(&mut a, t, &mut cv);
            let drawn = px(&cv);
            ui.invalidate();
            ui.paint(&mut a, t, &mut cv); // the same screen, drawn whole
            assert_eq!(drawn, px(&cv), "theme {theme}, word {pos}: the corner differs from a full redraw");
            t += 1_000;
        }
    }
}

#[test]
fn welcome_card_says_which_book_and_hides_the_sentence() {
    let text = "<!-- title: Cancroregina -->\n<!-- author: Tommaso Landolfi -->\n# Uno\nLa nave partì. Nessuno sapeva dove.";
    let mut a = App::new(Book::parse(text, "f"), CATALOG, Settings::default());
    let mut cv = Canvas::new(320, 240);
    let mut ui = Ui::new(320, 240);
    a.reader.seek(4, 0);
    ui.paint(&mut a, 0, &mut cv);
    let before: Vec<u32> = (0..320).map(|x| cv.pixel(x, ui.layout.context.cy() as i32)).collect();
    a.welcome_back(None, 10);
    ui.paint(&mut a, 10, &mut cv);
    let card: Vec<u32> = (0..320).map(|x| cv.pixel(x, ui.layout.context.cy() as i32)).collect();
    assert_ne!(before, card, "the card is drawn over the word and sentence");
    a.tap(Hit::Bookmark, 20); // any touch dismisses
    ui.paint(&mut a, 20, &mut cv);
    let after: Vec<u32> = (0..320).map(|x| cv.pixel(x, ui.layout.context.cy() as i32)).collect();
    assert_eq!(before, after, "and the reading screen comes back as it was");
}

#[test]
fn guide_lines_stay_put_from_word_to_word() {
    // Short words, a very long one (drawn smaller to fit) and a heading: the
    // two lines stay on the same rows, and no other full-width line appears.
    let text = "# Title here\nShort and Precipitevolissimevolmenteprecipitevolissimevolmente words, more words.";
    for theme in [0u8, 3] {
        let mut a = App::new(Book::new("t", text), CATALOG, Settings { theme, guides: true, mode: 0, chunk: 1, ..Settings::default() });
        let mut cv = Canvas::new(320, 240);
        let mut ui = Ui::new(320, 240);
        let l = ui.layout;
        let lines = |cv: &Canvas| -> Vec<i32> {
            let page = cv.pixel(160, l.word.y + 1);
            (l.word.y..l.word.bottom()).filter(|&y| (30..290).filter(|&x| cv.pixel(x, y) != page).count() > 240).collect()
        };
        let mut first = None;
        for i in 0..a.reader.book.len() {
            a.reader.seek(i, i as u64 * 10);
            ui.paint(&mut a, i as u64 * 10, &mut cv);
            let now = lines(&cv);
            match &first {
                None => first = Some(now),
                Some(f) => assert_eq!(&now, f, "theme {theme}, word {i}: the guide lines moved or doubled"),
            }
        }
    }
}

#[test]
fn lists_tables_and_pictures_together_draw_like_a_full_redraw() {
    // A list, a picture inside it, a table, a long list: played through in
    // every theme and reading mode. After every frame that changed, the
    // screen must equal the same moment drawn whole: nothing left behind,
    // nothing drawn over.
    let mut file = "Intro words before the list, see figure 1 below.\n\n\u{2022} first item\n\u{2022} second item\n\n{image:0}\n{caption} Figure 1. A gradient. {/caption}\n\n\u{2022} third item, like figure 1.\n\nThen a table.\n\n| Year | Work |\n|---|---|\n| 1503 | Mona Lisa |\n| 1498 | Last Supper |\n\n".as_bytes().to_vec();
    for k in 1..=30 {
        file.extend_from_slice(format!("{k}. step number {k}\n").as_bytes());
    }
    file.extend_from_slice(b"\nThe end.");
    file.extend_from_slice(reader_core::book::IMAGES_MAGIC);
    file.extend_from_slice(&1u16.to_le_bytes());
    file.extend_from_slice(&[0x40, 1, 0x82, 0]); // 320 x 130
    file.extend((0..320 * 130).flat_map(|i: u32| ((i % 320 / 16) as u16 * 0x0841).to_le_bytes()));
    let book = Book::from_bytes(&file, "t");
    assert_eq!((book.lists.len(), book.tables.len()), (2, 1), "the list goes on after its picture");
    for (theme, flash) in (0..4u8).flat_map(|t| [(t, false), (t, true)]) {
        for mode in [0u8, 1, 3] {
            let mut s = Settings { theme, mode, guides: true, ..Settings::default() };
            s.pacing.table_flash = flash;
            let mut a = App::new(Book::from_bytes(&file, "t"), CATALOG, s);
            let mut cv = Canvas::new(320, 240);
            let mut whole = Canvas::new(320, 240);
            let mut ui = Ui::new(320, 240);
            let mut fresh = Ui::new(320, 240);
            a.handle(reader_core::Key::Select, 0);
            let (mut t, mut frames, mut grids) = (0u64, 0, 0);
            while a.reader.is_playing() && t < 120_000 {
                t += 20;
                a.tick(t);
                if ui.paint(&mut a, t, &mut cv).is_empty() {
                    continue;
                }
                frames += 1;
                grids += a.grid_flash(t) as u32;
                if mode == 0 && a.reader.on_picture() {
                    // The picture in the list: its markers stay in the margin,
                    // and the picture keeps clear of them.
                    let l = ui.layout;
                    let amber = (l.word.y..l.word.bottom()).any(|y| (0..30).any(|x| matches!(cv.pixel(x, y), 0xF5C542 | 0xB8860B)));
                    assert!(amber, "theme {theme}: the current item's marker shows beside the picture");
                }
                fresh.invalidate();
                fresh.paint(&mut a, t, &mut whole);
                let diff = (0..240).flat_map(|y| (0..320).map(move |x| (x, y))).find(|&(x, y)| cv.pixel(x, y) != whole.pixel(x, y));
                assert!(diff.is_none(), "theme {theme}, mode {mode}, word {}: differs from a full redraw at {diff:?}", a.reader.pos());
            }
            assert!(frames > 20, "theme {theme}, mode {mode}: played");
            if mode == 0 {
                assert_eq!(grids > 0, flash, "theme {theme}: the grid flashes only in flash mode");
            }
        }
    }
}

#[test]
fn menu_icons_stay_inside_their_buttons_even_pressed() {
    // The cog (and the others) on the short title-bar buttons, pressed in
    // (its menu open) and nudged a pixel: no white icon pixel may reach the
    // button's last two rows or columns.
    for (w, h) in [(320, 240), (480, 320)] {
        for g in 0..4u8 {
            let mut a = App::new(Book::new("t", "# A\nOne two."), CATALOG, Settings { theme: 0, menu_colours: true, ..Settings::default() });
            let mut cv = Canvas::new(w, h);
            let mut ui = Ui::new(w, h);
            a.tap(Hit::Group(g), 0);
            a.tick(5_000);
            ui.paint(&mut a, 5_000, &mut cv);
            let r = reader_ui::layout::header_buttons(&ui.layout)[g as usize].rect;
            let bottom = (r.x..r.right()).flat_map(|x| (r.bottom() - 2..r.bottom()).map(move |y| (x, y)));
            let right = (r.y..r.bottom()).flat_map(|y| (r.right() - 2..r.right()).map(move |x| (x, y)));
            let touch = bottom.chain(right).find(|&(x, y)| cv.pixel(x, y) == 0xFFFFFF);
            assert!(touch.is_none(), "{w}x{h}, menu {g}: icon reaches the edge at {touch:?}");
        }
    }
}
