//! The reader in a web page (docs/demo): the same `reader-core` and
//! `reader-ui` as the device, compiled to WebAssembly. The page calls these
//! functions: it hands over books and touches, asks for a frame every
//! animation tick, and copies the pixels into a <canvas>. No browser API is
//! used from Rust, so there is no glue library: plain exported functions.

use std::cell::RefCell;

use reader_core::app::{Battery, BookInfo, DeviceInfo, Effect, Place};
use reader_core::gestures::{Event, Touch, HOLD_MS, REPEAT_MS};
use reader_core::{App, Book, Key, Settings};
use reader_ui::{Canvas, Ui, CATALOG};

struct Demo {
    app: App,
    ui: Ui,
    cv: Canvas,
    /// RGBA for the canvas, brightness applied.
    rgba: Vec<u8>,
    books: Vec<Vec<u8>>,
    /// How many of `books` came with the page (the rest were added).
    builtin: usize,
    /// An added book deleted on the reader: its index among the added ones,
    /// for the page to drop from its storage.
    deleted: Option<usize>,
    touch: Touch,
    finger: Option<(i32, i32)>,
    held: Option<(Key, u64, u64)>,
    /// Settings changed: the page saves them (localStorage).
    settings_dirty: bool,
    settings: Vec<u8>,
    redraw: bool,
}

thread_local! {
    static DEMO: RefCell<Option<Demo>> = const { RefCell::new(None) };
    /// Where the page writes a book before `add_book`.
    static INBOX: RefCell<Vec<u8>> = const { RefCell::new(Vec::new()) };
}

fn with<R>(f: impl FnOnce(&mut Demo) -> R) -> Option<R> {
    DEMO.with(|d| d.borrow_mut().as_mut().map(f))
}

/// Space for `len` bytes the page is about to write (a book or settings);
/// returns where.
#[no_mangle]
pub extern "C" fn inbox(len: usize) -> *mut u8 {
    INBOX.with(|b| {
        let mut b = b.borrow_mut();
        b.clear();
        b.resize(len, 0);
        b.as_mut_ptr()
    })
}

fn take_inbox() -> Vec<u8> {
    INBOX.with(|b| std::mem::take(&mut *b.borrow_mut()))
}

/// Starts the reader at `w`×`h`; the inbox may hold saved settings.
#[no_mangle]
pub extern "C" fn start(w: u32, h: u32) {
    let saved = take_inbox();
    let settings = Settings::from_bytes(&saved).unwrap_or_default();
    // Which brackets, quotes and dashes are asides: before any book is read.
    reader_core::book::set_aside_kinds(settings.asides);
    // A stand-in until the books arrive: its place is never saved over a
    // real book's (the first book's starting chapter, say).
    let mut loading = Book::new("Fast Reader", "# Fast Reader\nLoading…");
    loading.stand_in = true;
    let mut app = App::new(loading, CATALOG, settings);
    app.battery = Some(Battery { percent: 100, charging: false, millivolts: 4150 });
    app.device = DeviceInfo {
        board: "Web demo (the same code as the reader)".into(),
        firmware: format!("fastreader {}", env!("CARGO_PKG_VERSION")),
        touch: "mouse / touch".into(),
        free_memory_kb: 0,
        web: "this page".into(),
        ..Default::default()
    };
    app.library.clear();
    app.library_info.clear();
    DEMO.with(|d| {
        *d.borrow_mut() = Some(Demo {
            app,
            ui: Ui::new(w, h),
            cv: Canvas::new(w, h),
            rgba: vec![255; (w * h * 4) as usize],
            books: Vec::new(),
            builtin: 0,
            deleted: None,
            touch: Touch::default(),
            finger: None,
            held: None,
            settings_dirty: false,
            settings: Vec::new(),
            redraw: true,
        })
    });
}

/// A new screen size (window resized, phone turned, another display
/// chosen): the reader lays itself out again for it.
#[no_mangle]
pub extern "C" fn resize(w: u32, h: u32) {
    with(|d| {
        if (d.cv.w as u32, d.cv.h as u32) != (w, h) {
            d.ui = Ui::new(w, h);
            d.cv = Canvas::new(w, h);
            d.rgba = vec![255; (w * h * 4) as usize];
            d.redraw = true;
        }
    });
}

/// Adds the book in the inbox (converter output: markup and pictures) to
/// the library; the first one added is opened. `added` = 1 for a book the
/// visitor added (it can be deleted), 0 for one that came with the page
/// (those come first). Returns its index.
#[no_mangle]
pub extern "C" fn add_book(added: u32) -> u32 {
    let bytes = take_inbox();
    with(|d| {
        let book = Book::from_bytes(&bytes, "Book");
        let i = d.books.len();
        let size = bytes.len() as u32;
        d.books.push(bytes);
        if added == 0 {
            d.builtin = d.books.len();
        }
        d.app.library.push(book.title.clone());
        let place = if added == 0 { Place::Builtin } else { Place::Flash };
        d.app.library_info.push(BookInfo { place, bytes: size });
        d.app.positions.resize(d.app.library.len(), 0);
        // A first visit opens the demo's first book at its first chapter
        // ("I"), past the front matter (Looking-Glass's chess problem).
        if i == 0 && added == 0 && d.app.settings.pos == 0 {
            if let Some(c) = book.chapters().iter().find(|c| c.1 == "I") {
                d.app.positions[0] = c.0;
                // ...with a bookmark there (the folded corner), to come back to.
                if !d.app.bookmarks.contains(&(0, c.0)) {
                    d.app.bookmarks.push((0, c.0));
                }
            }
        }
        let wanted = d.app.settings.book as usize;
        if i == wanted || (i == 0 && wanted >= d.books.len()) || d.app.reader.book.len() < 3 {
            if i != wanted {
                d.app.settings.book = i as u8;
            }
            d.app.open_book(book, i as u8);
        }
        d.redraw = true;
        i as u32
    })
    .unwrap_or(0)
}

/// Where the finger is (in canvas pixels), or `down = 0` when lifted.
#[no_mangle]
pub extern "C" fn finger(x: i32, y: i32, down: u32) {
    with(|d| d.finger = (down != 0).then_some((x, y)));
}

/// A key: 0 up, 1 down, 2 left, 3 right, 4 select (play), 5 menu.
#[no_mangle]
pub extern "C" fn key(code: u32, down: u32, now: f64) {
    let k = match code {
        0 => Key::Up,
        1 => Key::Down,
        2 => Key::Left,
        3 => Key::Right,
        4 => Key::Select,
        5 => Key::Menu,
        _ => return,
    };
    let now = now as u64;
    with(|d| {
        if down != 0 {
            d.app.handle(k, now);
            d.held = Some((k, now, now));
        } else if d.held.is_some_and(|h| h.0 == k) {
            d.app.release(now);
            d.held = None;
        }
    });
}

/// One animation frame: input, timers, what the app asked for, drawing.
/// Returns 1 when the pixels changed (read them with `pixels`).
#[no_mangle]
pub extern "C" fn frame(now: f64) -> u32 {
    let now = now as u64;
    with(|d| {
        if let Some((k, t0, last)) = d.held {
            if k.repeats() && now - t0 >= HOLD_MS && now - last >= REPEAT_MS {
                d.app.handle_held(k, now);
                d.held = Some((k, t0, now));
            }
        }
        let (w, h) = (d.cv.w, d.cv.h);
        let flipped = d.app.settings.flipped;
        let hit = d.finger.map(|(x, y)| {
            let (x, y) = if flipped { (w - 1 - x, h - 1 - y) } else { (x, y) };
            d.ui.hit(&d.app, x, y)
        });
        match d.touch.update(hit, now) {
            Some(Event::Down(hit)) => d.app.touch_down(hit),
            Some(Event::Tap(hit)) => {
                d.app.tap(hit, now);
            }
            Some(Event::Held(k)) => {
                d.app.handle_held(k, now);
            }
            Some(Event::Release) => d.app.release(now),
            Some(Event::Cancel) => d.app.touch_cancel(),
            Some(Event::Press(k)) => {
                d.app.handle(k, now);
            }
            Some(Event::Boot(_)) => {} // the device's BOOT button only
            None => {}
        }
        d.app.tick(now);
        while let Some(e) = d.app.take_effect() {
            apply(d, e, now);
        }
        if d.redraw {
            d.ui.invalidate();
        }
        let changed = !d.ui.paint(&mut d.app, now, &mut d.cv).is_empty() || d.redraw;
        if changed {
            d.redraw = false;
            // Brightness is the reader's backlight; a page shows full colour.
            let dim = 255u32;
            for y in 0..h {
                for x in 0..w {
                    let c = d.cv.pixel(x, y);
                    let (sx, sy) = if flipped { (w - 1 - x, h - 1 - y) } else { (x, y) };
                    let o = ((sy * w + sx) * 4) as usize;
                    d.rgba[o] = ((c >> 16 & 0xFF) * dim / 255) as u8;
                    d.rgba[o + 1] = ((c >> 8 & 0xFF) * dim / 255) as u8;
                    d.rgba[o + 2] = ((c & 0xFF) * dim / 255) as u8;
                    d.rgba[o + 3] = 255;
                }
            }
            d.app.frame_shown(now);
        }
        changed as u32
    })
    .unwrap_or(0)
}

/// Carries out what the app asked for.
fn apply(d: &mut Demo, e: Effect, now: u64) {
    match e {
        Effect::SaveSettings | Effect::Display | Effect::SaveBookmarks => {
            d.settings_dirty = true;
            d.redraw = true;
        }
        Effect::OpenBook(i) => {
            if let Some(bytes) = d.books.get(i as usize) {
                let book = Book::from_bytes(bytes, "Book");
                d.app.open_book(book, i);
                d.settings_dirty = true;
            }
            d.redraw = true;
        }
        Effect::DeleteBook(i) => {
            let i = i as usize;
            if i >= d.builtin && i < d.books.len() {
                d.books.remove(i);
                d.app.book_removed(i, now);
                d.deleted = Some(i - d.builtin);
                d.settings_dirty = true;
                // The open book may have moved or gone: reopen.
                let open = (d.app.settings.book as usize).min(d.books.len().saturating_sub(1));
                if let Some(bytes) = d.books.get(open) {
                    let book = Book::from_bytes(bytes, "Book");
                    d.app.open_book(book, open as u8);
                }
            } else {
                d.app.cant_delete(now);
            }
            d.redraw = true;
        }
        // No sleep, radio or card in a web page.
        Effect::Sleep | Effect::Wifi(_) | Effect::FormatCard | Effect::Download | Effect::Dim(_) => {}
    }
}

/// An added book deleted on the reader since the last call: its index among
/// the added books, or -1.
#[no_mangle]
pub extern "C" fn take_deleted() -> i32 {
    with(|d| d.deleted.take().map_or(-1, |i| i as i32)).unwrap_or(-1)
}

/// The RGBA pixels of the last frame (width × height × 4 bytes).
#[no_mangle]
pub extern "C" fn pixels() -> *const u8 {
    with(|d| d.rgba.as_ptr()).unwrap_or(std::ptr::null())
}

/// When settings changed since the last call: their bytes for the page to
/// keep, else length 0. Read with `settings_ptr` right after.
#[no_mangle]
pub extern "C" fn settings_len() -> usize {
    with(|d| {
        if !d.settings_dirty {
            return 0;
        }
        d.settings_dirty = false;
        d.app.settings.pos = d.app.reader.pos() as u32;
        d.settings = d.app.settings.to_bytes().to_vec();
        d.settings.len()
    })
    .unwrap_or(0)
}

#[no_mangle]
pub extern "C" fn settings_ptr() -> *const u8 {
    with(|d| d.settings.as_ptr()).unwrap_or(std::ptr::null())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn load(text: &str) {
        let p = inbox(text.len());
        unsafe { std::ptr::copy_nonoverlapping(text.as_ptr(), p, text.len()) };
        add_book(0);
    }

    #[test]
    fn plays_a_book_and_draws() {
        inbox(0);
        start(320, 240);
        load("<!-- title: One -->\n# One\nHello there, reader of the web.");
        load("<!-- title: Two -->\nSecond book.");
        assert_eq!(frame(0.0), 1, "first frame drawn");
        with(|d| assert_eq!(d.app.reader.book.title, "One"));
        key(4, 1, 10.0); // play
        key(4, 0, 20.0);
        let mut changed = 0;
        for t in 1..200 {
            changed += frame(t as f64 * 20.0);
        }
        assert!(changed > 3, "words went by");
        let px = unsafe { std::slice::from_raw_parts(pixels(), 320 * 240 * 4) };
        assert!(px.chunks(4).any(|c| c[0] < 100), "something dark was drawn");
        // Another screen: laid out again at its size.
        resize(960, 540);
        assert_eq!(frame(4_000.0), 1);
        with(|d| assert_eq!((d.cv.w, d.cv.h), (960, 540)));
        // An added book can be deleted (the page hears which); built-in ones can't.
        let mine = "<!-- title: Mine -->\nMy own book.";
        let p = inbox(mine.len());
        unsafe { std::ptr::copy_nonoverlapping(mine.as_ptr(), p, mine.len()) };
        assert_eq!(add_book(1), 2);
        with(|d| apply(d, Effect::DeleteBook(0), 5_000));
        assert_eq!(take_deleted(), -1, "a built-in book stays");
        with(|d| apply(d, Effect::DeleteBook(2), 5_100));
        assert_eq!(take_deleted(), 0, "the first added book went");
        with(|d| assert_eq!(d.app.library.len(), 2));
    }
}
