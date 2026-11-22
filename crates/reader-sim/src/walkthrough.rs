//! Scripted walkthrough: drives every function the way a finger would (find
//! the drawn button, tap its centre through the hit test) and saves a PNG
//! after each step. Used for visual review and as an end-to-end check: a
//! step whose button can't be found or that doesn't change the screen panics.

use std::path::Path;

use reader_core::app::{Battery, PickKind, Screen};
use reader_core::{App, Book, Hit, Key, Settings};
use reader_ui::layout::{header_buttons, list_geom, toolbar, Icon, RowKind};
use reader_ui::{Canvas, Ui, CATALOG};

use crate::{sim_device, write_png};

struct Run<'a> {
    app: App,
    ui: Ui,
    cv: Canvas,
    now: u64,
    n: usize,
    dir: &'a Path,
    prefix: String,
}

impl Run<'_> {
    fn shot(&mut self, name: &str) {
        self.ui.invalidate();
        self.ui.paint(&mut self.app, self.now, &mut self.cv);
        self.n += 1;
        write_png(&self.dir.join(format!("{}{:02}-{name}.png", self.prefix, self.n)), &self.cv);
    }

    fn wait(&mut self, ms: u64) {
        let end = self.now + ms;
        while self.now < end {
            self.now += 10;
            self.app.tick(self.now);
        }
    }

    /// Taps a drawn toolbar or header button by its icon, through hit testing.
    fn tap_icon(&mut self, icon: Icon) {
        let l = self.ui.layout;
        let b = if self.app.screen.is_overlay() {
            let g = list_geom(&self.app, &l);
            [Some(g.back), Some(g.close), g.scroll.map(|s| s.0), g.scroll.map(|s| s.1)]
                .into_iter()
                .flatten()
                .find(|b| b.icon == icon)
        } else {
            let mut bs = toolbar(&self.app, &l).0;
            bs.extend(header_buttons(&l));
            bs.into_iter().find(|b| b.icon == icon)
        }
        .unwrap_or_else(|| panic!("no {icon:?} button on {:?}", self.app.screen));
        let hit = self.ui.hit(&self.app, b.rect.cx() as i32, b.rect.cy() as i32).expect("button not hittable");
        self.app.touch_down(Some(hit));
        self.app.tap(hit, self.now);
        self.wait(30);
    }

    /// Taps a list row by its visible label.
    fn tap_row(&mut self, label: &str) {
        let l = self.ui.layout;
        // In another menu: close this one and tap that menu's button.
        if let Screen::Menu { .. } = self.app.screen {
            if let Some(i) = (0..reader_core::app::MENU_LEN).find(|&i| reader_core::app::menu_item(i).label() == label) {
                let (g, _) = reader_core::app::menu_group(i);
                let here = match self.app.screen { Screen::Menu { sel } => reader_core::app::menu_group(sel).0, _ => g };
                if g != here {
                    self.tap_icon(Icon::Close);
                    self.tap_icon([Icon::Gear, Icon::Book, Icon::Library, Icon::Text][g]);
                    if !matches!(self.app.screen, Screen::Menu { .. }) {
                        return; // the Library button opens the list itself
                    }
                }
            }
        }
        // Like a person: if the row isn't on screen, press the drawn ▼ until it is.
        let mut tries = 0;
        let row = loop {
            let g = list_geom(&self.app, &l);
            if let Some(r) = g.rows.iter().find(|r| matches!(r.kind, RowKind::Item(i) if reader_ui::layout::row_text(&self.app, i).0 == label)) {
                break r.clone();
            }
            tries += 1;
            assert!(tries < 6 && g.scroll.is_some(), "row {label:?} unreachable on {:?}", self.app.screen);
            let len = match self.app.screen {
                Screen::Pick { kind, .. } => self.app.pick_len(kind),
                _ => reader_core::app::MENU_LEN,
            };
            let target = (0..len).find(|&i| reader_ui::layout::row_text(&self.app, i).0 == label).expect("no such row");
            let sel = match self.app.screen {
                Screen::Menu { sel } | Screen::Pick { sel, .. } => sel,
                _ => 0,
            };
            self.tap_icon(if target > sel { Icon::Down } else { Icon::Up });
        };
        let hit = self.ui.hit(&self.app, row.rect.cx() as i32, row.rect.cy() as i32).expect("row not hittable");
        assert!(matches!(hit, Hit::Row(_)));
        self.app.tap(hit, self.now);
        self.wait(30);
    }

    fn key(&mut self, k: Key) {
        self.app.handle(k, self.now);
        self.wait(30);
    }
}

fn new_app(text: &str, s: Settings) -> App {
    let mut a = App::new(Book::new("The Quiet Machine", text), CATALOG, s);
    a.battery = Some(Battery { percent: 82, charging: false, millivolts: 4010 });
    a.device = sim_device();
    a
}

pub fn run(dir: &Path, (w, h): (u32, u32)) {
    std::fs::create_dir_all(dir).unwrap();
    let text = std::fs::read_to_string("books/sample.md").expect("books/sample.md");

    // 1. The full story, in Vista.
    let mut r = Run { app: new_app(&text, Settings::default()), ui: Ui::new(w, h), cv: Canvas::new(w, h), now: 0, n: 0, dir, prefix: "flow-".into() };
    r.shot("start-paused-on-chapter-title");
    r.tap_icon(Icon::Play);
    r.shot("play-tapped-toast-reading");
    r.wait(2_600);
    r.shot("reading-mid-text");
    r.tap_icon(Icon::Plus);
    r.shot("faster-toast");
    r.tap_icon(Icon::Minus);
    r.tap_icon(Icon::Minus);
    r.shot("slower-twice");
    r.tap_icon(Icon::SentenceBack);
    r.shot("back-one-sentence");
    r.app.handle_held(Key::Left, r.now);
    r.app.handle_held(Key::Left, r.now + 110);
    r.app.handle_held(Key::Left, r.now + 220);
    r.shot("hold-back-line-view");
    r.app.release(r.now + 300);
    r.wait(300);
    r.shot("released-reading-again");
    r.tap_icon(Icon::Pause);
    r.wait(1500);
    r.shot("paused-with-sentence-context");
    r.tap_icon(Icon::Book);
    r.shot("menu-book");
    r.app.library = vec!["The Quiet Machine".into(), "La storia dell'arte".into()];
    r.app.library_info = vec![
        reader_core::app::BookInfo { place: reader_core::app::Place::Card, bytes: 1_200 },
        reader_core::app::BookInfo { place: reader_core::app::Place::Card, bytes: 29_601_778 },
    ];
    r.tap_row("Delete a book…");
    r.shot("delete-which");
    r.key(Key::Select);
    r.key(Key::Down);
    r.shot("delete-confirm");
    r.tap_icon(Icon::Back);
    r.shot("delete-back-to-list");
    r.tap_icon(Icon::Close);
    r.tap_icon(Icon::Book);
    r.tap_icon(Icon::Close);
    r.tap_icon(Icon::Gear);
    r.shot("menu-settings");
    r.tap_icon(Icon::Close);
    r.tap_icon(Icon::Library);
    r.shot("menu-library");
    r.tap_icon(Icon::Back);
    r.tap_icon(Icon::Text);
    r.shot("menu-text");
    r.tap_icon(Icon::Down);
    r.shot("menu-text-scrolled");
    r.tap_row("Font");
    r.shot("font-list");
    r.key(Key::Down);
    r.shot("font-list-live-preview");
    r.tap_icon(Icon::Back);
    r.shot("back-kept-font");
    r.tap_row("Words per flash");
    r.tap_row("2 words");
    r.shot("words-per-flash-set");
    r.tap_icon(Icon::Close);
    r.shot("two-words-per-flash");
    r.tap_icon(Icon::Book);
    r.tap_row("Text size");
    r.tap_row("Large");
    r.tap_icon(Icon::Close);
    r.shot("large-text");
    r.tap_icon(Icon::Book);
    r.tap_row("Text size");
    r.tap_row("Normal");
    r.tap_row("Reading mode");
    r.shot("mode-list");
    r.tap_row("Phrases");
    r.tap_icon(Icon::Play);
    r.wait(2_000);
    r.shot("mode-phrases-playing");
    r.tap_icon(Icon::Pause);
    r.tap_icon(Icon::Book);
    r.tap_row("Reading mode");
    r.tap_row("Ticker");
    r.tap_icon(Icon::Play);
    r.wait(1_730);
    r.shot("mode-ticker-gliding");
    r.tap_icon(Icon::Pause);
    r.tap_icon(Icon::Book);
    r.tap_row("Reading mode");
    r.tap_row("Page");
    r.shot("mode-page-highlight");
    r.tap_icon(Icon::Play);
    r.wait(2_500);
    r.shot("mode-page-highlight-moving");
    r.tap_icon(Icon::Pause);
    r.tap_icon(Icon::PageForward);
    r.shot("page-next");
    let dot = reader_ui::layout::highlight_button(&r.ui.layout);
    let hit = r.ui.hit(&r.app, dot.rect.cx() as i32, dot.rect.cy() as i32).expect("dot");
    r.app.tap(hit, r.now);
    r.wait(1_500);
    r.shot("page-highlight-off");
    r.tap_icon(Icon::Book);
    r.tap_row("Paragraphs");
    r.tap_row("Indented");
    r.tap_row("Text alignment");
    r.tap_row("Justified");
    r.tap_row("Line spacing");
    r.tap_row("Airy");
    r.tap_icon(Icon::Close);
    r.wait(1_500);
    r.shot("page-book-style-justified-airy");
    r.tap_icon(Icon::Book);
    r.tap_row("Reading mode");
    r.tap_row("Word by word");
    // Bookmark: tap the page corner (dog-ear), in reading and page view.
    let corner = reader_ui::layout::bookmark_corner(&r.app, &r.ui.layout);
    let hit = r.ui.hit(&r.app, corner.rect.cx() as i32, corner.rect.cy() as i32).expect("corner");
    r.app.tap(hit, r.now);
    r.shot("bookmarked-page-corner");
    r.wait(1_500);
    // Parentheses as side marks.
    let paren = (0..r.app.reader.book.len()).find(|&i| r.app.reader.book.word(i).unwrap().style.bracketed()).unwrap();
    r.app.reader.seek(paren + 2, r.now);
    r.app.set_setting("mode", 0).unwrap();
    r.app.screen = Screen::Reading;
    r.shot("parentheses-side-marks");
    r.app.set_setting("mode", 1).unwrap();
    r.shot("parentheses-side-marks-phrases");
    r.app.set_setting("mode", 3).unwrap();
    r.app.screen = Screen::Page;
    r.app.reader.seek(paren, r.now);
    r.tap_icon(Icon::Book);
    r.tap_icon(Icon::Close);
    r.wait(1_500);
    r.shot("page-with-parentheses");
    r.app.set_setting("mode", 0).unwrap();
    r.app.screen = Screen::Reading;
    r.tap_icon(Icon::Book);
    r.tap_row("Bookmarks");
    r.shot("bookmarks-list");
    r.tap_icon(Icon::Close);
    r.tap_icon(Icon::Book);
    r.tap_row("Chapters");
    r.shot("chapters-list");
    r.tap_row("Chapter Two: Speed");
    r.shot("jumped-to-chapter-two");
    r.tap_icon(Icon::Book);
    r.tap_row("Device info");
    r.shot("device-info");
    r.key(Key::Menu);
    for _ in 0..40 {
        r.tap_icon(Icon::Plus);
    }
    r.shot("limit-fastest-amber");
    r.app.reader.seek(r.app.reader.book.len() - 2, r.now);
    r.app.reader.set_wpm(600);
    r.tap_icon(Icon::Play);
    r.wait(3_000);
    r.shot("finished");

    // Alignment: a short, a medium and a long word, then phrases.
    r.app.open_book(reader_core::Book::new("Align", "Io contemporaneamente leggevo (un libro «strano» davvero) la differenza tra parole lunghissime e brevi."), 0);
    r.app.set_setting("mode", 0).unwrap();
    r.app.screen = Screen::Reading;
    r.app.settings.context = false;
    for (i, name) in [(0, "align-short-word"), (1, "align-long-word"), (2, "align-medium-word"), (5, "align-guillemets")] {
        r.app.reader.seek(i, r.now);
        r.shot(name);
    }
    r.app.set_setting("mode", 1).unwrap();
    for (i, name) in [(0, "align-phrase-1"), (3, "align-phrase-2"), (8, "align-phrase-3")] {
        r.app.reader.seek(i, r.now);
        r.shot(name);
    }
    r.app.settings.context = true;

    // A long sentence under the word: ▲▼ to scroll it; a long word in two flashes.
    r.app.open_book(reader_core::Book::new("Long", "Era contemporaneamente una frase molto lunga che continua senza fermarsi mai, con tante parole che non stanno nelle due righe sotto la parola, e prosegue ancora fino alla fine del suo lungo discorso."), 0);
    r.app.set_setting("mode", 0).unwrap();
    r.app.set_setting("position", 1).unwrap();
    r.app.set_setting("chunk", 1).unwrap();
    r.app.screen = Screen::Reading;
    r.app.reader.seek(1, r.now);
    r.shot("long-word-first-half");
    r.app.reader.play(r.now);
    while r.app.reader.split_part() != Some(true) {
        r.wait(10);
    }
    r.app.reader.pause();
    r.shot("long-word-second-half");
    r.app.reader.seek(12, r.now);
    r.shot("long-sentence-arrows");
    let [_, down] = reader_ui::layout::context_arrows(&r.ui.layout);
    if let Some(h) = r.ui.hit(&r.app, down.cx() as i32, down.cy() as i32) {
        r.app.tap(h, r.now);
    }
    r.shot("long-sentence-scrolled");
    r.app.set_setting("position", 0).unwrap();
    r.tap_icon(Icon::Text);
    r.tap_row("Picture time");
    r.shot("picture-time-list");
    r.tap_icon(Icon::Close);

    // Pictures: a book with a chart, in word view and page view.
    let mut file = b"# Figures\n\nThe chart below shows reading speed by week.\n\n{image:0}\n\nAfter the chart the text goes on as usual.".to_vec();
    file.extend_from_slice(reader_core::book::IMAGES_MAGIC);
    let (pw, ph) = (240u16, 150u16);
    file.extend_from_slice(&1u16.to_le_bytes());
    file.extend_from_slice(&pw.to_le_bytes());
    file.extend_from_slice(&ph.to_le_bytes());
    for y in 0..ph {
        for x in 0..pw {
            let bar = x / 40;
            let top = 140 - [40, 70, 55, 95, 80, 120][bar.min(5) as usize];
            let v: u16 = if y >= 140 || x < 4 { 0x2104 } else if y > top && x % 40 > 8 && x % 40 < 34 { 0x3B7F } else { 0xFFFF };
            file.extend_from_slice(&v.to_le_bytes());
        }
    }
    r.app.open_book(reader_core::Book::from_bytes(&file, "Figures"), 0);
    r.app.set_setting("mode", 0).unwrap();
    r.app.screen = Screen::Reading;
    r.app.reader.seek(r.app.reader.book.len() - 10, r.now);
    r.shot("picture-word-view");
    r.app.set_setting("mode", 3).unwrap();
    r.app.screen = Screen::Page;
    r.app.reader.seek(0, r.now);
    r.shot("picture-page-view");

    // 2. The same key screens in every theme.
    for theme in 0..CATALOG.themes.len() as u8 {
        let slug = CATALOG.themes[theme as usize].to_lowercase().replace(' ', "-");
        let mut r = Run { app: new_app(&text, Settings { theme, ..Settings::default() }), ui: Ui::new(w, h), cv: Canvas::new(w, h), now: 0, n: 0, dir, prefix: format!("{slug}-") };
        r.app.reader.seek(30, 0);
        r.shot("paused");
        r.app.tap(reader_core::app::Hit::Bookmark, r.now);
        r.wait(1600);
        r.shot("bookmarked");
        r.app.tap(reader_core::app::Hit::Bookmark, r.now);
        r.tap_icon(Icon::Play);
        r.wait(1600);
        r.shot("playing");
        r.tap_icon(Icon::Plus);
        r.shot("toast");
        r.app.handle_held(Key::Left, r.now);
        r.shot("line");
        r.app.release(r.now);
        r.tap_icon(Icon::Book);
        r.shot("menu");
        r.app.screen = Screen::Pick { kind: PickKind::Font, sel: 1, original: 0 };
        r.shot("fonts");
        r.key(Key::Menu);
        r.app.set_setting("mode", 3).unwrap();
        r.app.screen = Screen::Page;
        r.shot("page");
        r.app.set_setting("mode", 1).unwrap();
        r.app.screen = Screen::Reading;
        r.shot("phrases");
    }
    println!("wrote walkthrough to {}", dir.display());
}
