//! Desktop simulator for the fast reader.
//!
//!   reader-sim [book.md]                                window (sharp, 960x720) + web view
//!   reader-sim [book.md] --device                       the reader's real 320x240 pixels, shown 2x
//!   reader-sim [book.md] [--size WxH] [--scale 1|2|4]   any size; --lan lets phones on the network in
//!   reader-sim walkthrough <dir> [--size WxH]           PNG of every function, every theme
//!
//! The mouse is a finger: press lights a button, release acts, slide off
//! cancels, hold ◀◀ / ▶▶ / − / + repeats. Keyboard: arrows, Space/Enter,
//! M or Esc for the menu. The web view runs at http://localhost:8080.
//! `--headless`: only the web view, no window.

mod update;
mod walkthrough;
mod web;

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use minifb::{Key as K, KeyRepeat, MouseButton, MouseMode, Scale, Window, WindowOptions};
use reader_core::app::{Battery, DeviceInfo};
use reader_core::gestures::{Event, Touch, HOLD_MS, REPEAT_MS};
use reader_core::{App, Book, Effect, Key, Settings};
use reader_ui::color::from565;
use reader_ui::{Canvas, Ui, CATALOG};

fn arg(args: &[String], name: &str) -> Option<String> {
    args.iter().position(|a| a == name).and_then(|i| args.get(i + 1)).cloned()
}

/// Run as FastReader.app (the Mac installer): books, places and settings
/// live in ~/Library/Application Support/FastReader, and the sample books
/// inside the app are put there the first time.
fn use_app_folder() -> bool {
    let Ok(exe) = std::env::current_exe() else { return false };
    let Some(contents) = exe.ancestors().find(|p| p.file_name().is_some_and(|n| n == "Contents")) else { return false };
    if contents.parent().is_none_or(|a| a.extension().is_none_or(|e| e != "app")) {
        return false;
    }
    let Some(home) = std::env::var_os("HOME") else { return false };
    let dir = PathBuf::from(home).join("Library/Application Support/FastReader");
    for sub in ["books", "books/uploaded", "target"] {
        let _ = std::fs::create_dir_all(dir.join(sub));
    }
    if let Ok(found) = std::fs::read_dir(contents.join("Resources/books")) {
        for e in found.flatten() {
            let to = dir.join("books").join(e.file_name());
            if !to.exists() {
                let _ = std::fs::copy(e.path(), to);
            }
        }
    }
    let _ = std::env::set_current_dir(&dir);
    true
}

/// Running as the installed Mac app (it checks for updates).
static AS_APP: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

fn main() {
    AS_APP.store(use_app_folder(), std::sync::atomic::Ordering::Relaxed);
    let args: Vec<String> = std::env::args().skip(1).collect();
    let size = arg(&args, "--size")
        .and_then(|s| s.split_once('x').and_then(|(w, h)| Some((w.parse().ok()?, h.parse().ok()?))))
        .unwrap_or((320u32, 240u32));
    let positional: Vec<&String> = args
        .iter()
        .enumerate()
        .filter(|(i, a)| !a.starts_with("--") && (*i == 0 || !args[i - 1].starts_with("--")))
        .map(|(_, a)| a)
        .collect();
    if positional.first().map(|s| s.as_str()) == Some("record") {
        // record <book> <dir> [--theme N] [--wpm N] [--blue] [--demo]: frames for a GIF.
        let book = PathBuf::from(positional.get(1).expect("record <book> <dir>").as_str());
        let dir = PathBuf::from(positional.get(2).expect("record <book> <dir>").as_str());
        let mut s = Settings::default();
        if let Some(t) = arg(&args, "--theme").and_then(|v| v.parse().ok()) {
            s.theme = t;
        }
        if let Some(w) = arg(&args, "--wpm").and_then(|v| v.parse().ok()) {
            s.wpm = w;
        }
        s.blue_blocks = args.iter().any(|a| a == "--blue");
        walkthrough::record(&book, &dir, size, s, args.iter().any(|a| a == "--demo"));
        return;
    }
    if positional.first().map(|s| s.as_str()) == Some("walkthrough") {
        let dir = positional.get(1).map(PathBuf::from).unwrap_or_else(|| "walkthrough".into());
        walkthrough::run(&dir, size);
        return;
    }
    let book = positional.first().map(PathBuf::from);
    // The window: drawn sharp at 960x720 unless asked otherwise (everything
    // scales with the screen); --device shows the reader's own pixels.
    let device = args.iter().any(|a| a == "--device");
    let (size, default_scale) = if device || arg(&args, "--size").is_some() { (size, Scale::X2) } else { ((960, 720), Scale::X1) };
    let scale = match arg(&args, "--scale").as_deref() {
        Some("1") => Scale::X1,
        Some("2") => Scale::X2,
        Some("4") => Scale::X4,
        _ => default_scale,
    };
    let lan = args.iter().any(|a| a == "--lan");
    run_window(book, size, scale, args.iter().any(|a| a == "--headless"), lan);
}

pub fn load_book(path: &Path) -> Book {
    let bytes = std::fs::read(path).unwrap_or_else(|e| {
        eprintln!("could not read {}: {e}; using a placeholder", path.display());
        b"# No book\nPass a .md or .txt file as the first argument.".to_vec()
    });
    let title = path.file_stem().and_then(|s| s.to_str()).unwrap_or("Book").replace(['_', '-'], " ");
    // Text, then optionally the image section (converter / upload output).
    Book::from_bytes(&bytes, &title)
}

/// Every book in `books/` and `books/private/` (converted with tools/convert.py).
fn library(first: Option<PathBuf>) -> Vec<PathBuf> {
    let mut v: Vec<PathBuf> = first.into_iter().collect();
    for dir in ["books", "books/private", UPLOADS, "target/sim-card/BOOKS"] {
        let mut found: Vec<PathBuf> = std::fs::read_dir(dir)
            .into_iter()
            .flatten()
            .flatten()
            .map(|e| e.path())
            .filter(|p| matches!(p.extension().and_then(|e| e.to_str()), Some("md" | "txt" | "BK" | "bk")))
            .collect();
        found.sort();
        for p in found {
            if !v.iter().any(|q| q.canonicalize().ok() == p.canonicalize().ok()) {
                v.push(p);
            }
        }
    }
    v
}

fn title_of(path: &Path) -> String {
    let bytes = std::fs::read(path).unwrap_or_default();
    let text = String::from_utf8_lossy(&bytes[..bytes.len().min(400)]).into_owned();
    let head: String = text.chars().take(300).collect();
    head.strip_prefix("<!-- title:")
        .and_then(|r| r.split_once("-->"))
        .map(|(t, _)| t.trim().to_string())
        .unwrap_or_else(|| path.file_stem().and_then(|s| s.to_str()).unwrap_or("Book").replace(['_', '-'], " "))
}

/// Where books sent from the browser land (like the device's flash area).
pub const UPLOADS: &str = "books/uploaded";
/// The simulator's SD card: a folder.
pub const SIM_CARD: &str = "target/sim-card";

pub fn sim_card() -> reader_core::app::Card {
    reader_core::app::Card { total_mb: 30_436, free_mb: 30_100, formatted: true }
}

fn bookmarks_path() -> PathBuf {
    PathBuf::from("target/sim-bookmarks.txt")
}

fn positions_path() -> PathBuf {
    PathBuf::from("target/sim-positions.txt")
}

pub fn sim_device() -> DeviceInfo {
    DeviceInfo {
        board: "Simulator (ES3C28P layout)".into(),
        firmware: format!("fastreader {}", env!("CARGO_PKG_VERSION")),
        touch: "mouse".into(),
        free_memory_kb: 7_900,
        web: "http://localhost:8080 (simulator)".into(),
        home_ssid: String::new(),
        home: "not available in the simulator".into(),
    }
}

fn settings_path() -> PathBuf {
    PathBuf::from("target/sim-settings.bin")
}

/// Converts the canvas to what the panel shows: dithered RGB565, dimmed by
/// the brightness setting, turned 180° if flipped.
fn present(cv: &Canvas, out: &mut [u32], s: &Settings) {
    let (w, h) = (cv.w, cv.h);
    let dim = [90u32, 140, 185, 225, 255][(s.brightness.clamp(1, 5) - 1) as usize];
    for (i, p) in cv.rgb565(cv.bounds()).enumerate() {
        let c = from565(p);
        let scale = |v: u32| v * dim / 255;
        let c = scale(c >> 16 & 0xFF) << 16 | scale(c >> 8 & 0xFF) << 8 | scale(c & 0xFF);
        let (x, y) = (i as i32 % w, i as i32 / w);
        let (x, y) = if s.flipped { (w - 1 - x, h - 1 - y) } else { (x, y) };
        out[(y * w + x) as usize] = c;
    }
}

/// Carries out what the app asked for (saving, opening books...). Returns
/// true when the whole screen must be redrawn.
fn apply_effects(a: &mut App, books: &Arc<Mutex<Vec<PathBuf>>>) -> bool {
    let mut redraw = false;
    while let Some(e) = a.take_effect() {
        match e {
            Effect::SaveSettings => {
                let _ = std::fs::write(settings_path(), a.settings.to_bytes());
            }
            Effect::Display => {
                redraw = true;
                let _ = std::fs::write(settings_path(), a.settings.to_bytes());
            }
            Effect::Sleep => println!("(the device would sleep now; any button wakes it)"),
            Effect::Download => {
                let _ = std::process::Command::new("open").arg(update::RELEASES).spawn();
            }
            Effect::SaveBookmarks => {
                let list: Vec<String> = a.bookmarks.iter().map(|(b, w)| format!("{b}:{w}")).collect();
                let _ = std::fs::write(bookmarks_path(), list.join(" "));
            }
            Effect::DeleteBook(i) => {
                // Only uploaded or card books; the simulator's book folders stay.
                let mut list = books.lock().unwrap();
                let i = i as usize;
                match list.get(i) {
                    Some(p) if p.starts_with(UPLOADS) || p.starts_with(SIM_CARD) => {
                        let _ = std::fs::remove_file(p);
                        list.remove(i);
                        a.book_removed(i, 0);
                    }
                    _ => a.cant_delete(0),
                }
            }
            Effect::FormatCard => {
                // The simulator's card is a folder: formatting empties it.
                let _ = std::fs::remove_dir_all(SIM_CARD);
                let _ = std::fs::create_dir_all(format!("{SIM_CARD}/BOOKS"));
                let mut list = books.lock().unwrap();
                list.retain(|p| !p.starts_with(SIM_CARD));
                let keep = list.len();
                drop(list);
                a.books_removed(keep, 0);
                a.card_formatted(Some(sim_card()), 0);
            }
            Effect::Wifi(on) => println!("(web view {}: the simulator always serves http://localhost:8080)", if on { "on" } else { "off" }),
            Effect::OpenBook(i) => {
                let path = books.lock().unwrap().get(i as usize).cloned().unwrap_or_default();
                a.open_book(load_book(&path), i);
                let list: Vec<String> = a.positions.iter().map(|p| p.to_string()).collect();
                let _ = std::fs::write(positions_path(), list.join(" "));
                redraw = true;
            }
        }
    }
    redraw
}

fn run_window(first: Option<PathBuf>, (w, h): (u32, u32), scale: Scale, headless: bool, lan: bool) {
    let books = Arc::new(Mutex::new(library(first)));
    let first_count = books.lock().unwrap().len();
    let mut settings = std::fs::read(settings_path()).ok().and_then(|b| Settings::from_bytes(&b)).unwrap_or_default();
    let list = books.lock().unwrap().clone();
    settings.book = settings.book.min(list.len().saturating_sub(1) as u8);
    let mut app = App::new(load_book(&list[settings.book as usize]), CATALOG, settings);
    app.library = list.iter().map(|p| title_of(p)).collect();
    app.library_info = list
        .iter()
        .map(|p| {
            use reader_core::app::{BookInfo, Place};
            let place = if p.starts_with(SIM_CARD) { Place::Card } else if p.starts_with(UPLOADS) { Place::Flash } else { Place::Builtin };
            BookInfo { place, bytes: std::fs::metadata(p).map_or(0, |m| m.len() as u32) }
        })
        .collect();
    app.positions = std::fs::read_to_string(positions_path())
        .unwrap_or_default()
        .split_whitespace()
        .filter_map(|n| n.parse().ok())
        .collect();
    app.positions.resize(list.len(), 0);
    app.welcome_back(None, 0); // as the reader does when switched on
    app.bookmarks = std::fs::read_to_string(bookmarks_path())
        .unwrap_or_default()
        .split_whitespace()
        .filter_map(|e| e.split_once(':').and_then(|(b, w)| Some((b.parse().ok()?, w.parse().ok()?))))
        .collect();
    let _ = first_count;
    app.battery = Some(Battery { percent: 82, charging: false, millivolts: 4010 });
    app.device = sim_device();
    let _ = std::fs::create_dir_all(format!("{SIM_CARD}/BOOKS"));
    app.card = Some(sim_card());
    let app = Arc::new(Mutex::new(app));
    if AS_APP.load(std::sync::atomic::Ordering::Relaxed) || std::env::var_os("FASTREADER_CHECK_UPDATES").is_some() {
        update::check(app.clone());
    }
    let start = Instant::now();
    // --lan: phones and other computers on the same network can open it too.
    web::serve(app.clone(), books.clone(), if lan { "0.0.0.0:8080" } else { "127.0.0.1:8080" }, start);
    println!("Web view: http://localhost:8080");
    if lan {
        println!("From a phone on the same Wi-Fi: http://<this computer's address>:8080");
    }
    if headless {
        // Only the web view and the reader's clock: no window.
        loop {
            let now = start.elapsed().as_millis() as u64;
            {
                let mut a = app.lock().unwrap();
                a.tick(now);
                apply_effects(&mut a, &books);
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
    }

    let (mut w, mut h) = (w, h);
    let mut cv = Canvas::new(w, h);
    let mut ui = Ui::new(w, h);
    // Drawn at the window's own size (not scaled up): drag its edge or
    // maximize it and the reader lays itself out for the new screen, like a
    // different display would. --device keeps the reader's fixed pixels.
    let resizable = matches!(scale, Scale::X1);
    let mut window = Window::new(
        "Fast Reader",
        w as usize,
        h as usize,
        WindowOptions { scale, resize: resizable, scale_mode: minifb::ScaleMode::Stretch, ..WindowOptions::default() },
    )
    .expect("window");
    if resizable {
        println!("Window: drag its edge or maximize it; the reader fills any size.");
    }
    let mut out = vec![0u32; (w * h) as usize];
    let mut held: Option<(Key, u64, u64)> = None;
    let mut touch = Touch::default();
    let mut dirty = true;

    while window.is_open() {
        let now = start.elapsed().as_millis() as u64;
        if resizable {
            let (nw, nh) = window.get_size();
            let (nw, nh) = (nw.max(200) as u32, nh.max(150) as u32);
            if (nw, nh) != (w, h) {
                (w, h) = (nw, nh);
                cv = Canvas::new(w, h);
                ui = Ui::new(w, h);
                out = vec![0u32; (w * h) as usize];
                dirty = true;
                println!("Window: now {w}x{h}");
            }
        }
        {
            let mut a = app.lock().unwrap();
            for k in window.get_keys_pressed(KeyRepeat::No) {
                if let Some(key) = map_key(k) {
                    a.handle(key, now);
                    held = Some((key, now, now));
                }
            }
            if let Some((key, t0, last)) = held {
                if !window.get_keys().iter().any(|k| map_key(*k) == Some(key)) {
                    a.release(now);
                    held = None;
                } else if key.repeats() && now - t0 >= HOLD_MS && now - last >= REPEAT_MS {
                    a.handle_held(key, now);
                    held = Some((key, t0, now));
                }
            }
            // Mouse as a finger. minifb reports mouse positions in buffer pixels.
            let finger = window
                .get_mouse_down(MouseButton::Left)
                .then(|| window.get_mouse_pos(MouseMode::Discard))
                .flatten()
                .map(|(x, y)| {
                    let (x, y) = if a.settings.flipped { (w as f32 - 1.0 - x, h as f32 - 1.0 - y) } else { (x, y) };
                    ui.hit(&a, x as i32, y as i32)
                });
            match touch.update(finger, now) {
                Some(Event::Down(hit)) => a.touch_down(hit),
                Some(Event::Tap(hit)) => {
                    a.tap(hit, now);
                }
                Some(Event::Held(k)) => {
                    a.handle_held(k, now);
                }
                Some(Event::Release) => a.release(now),
                Some(Event::Cancel) => a.touch_cancel(),
                Some(Event::Press(k)) => {
                    a.handle(k, now);
                }
                None => {}
            }
            a.tick(now);
            if apply_effects(&mut a, &books) {
                ui.invalidate();
                dirty = true;
            }
            if !ui.paint(&mut a, now, &mut cv).is_empty() || dirty {
                present(&cv, &mut out, &a.settings);
                a.frame_shown(start.elapsed().as_millis() as u64);
                dirty = false;
            }
        }
        window.update_with_buffer(&out, w as usize, h as usize).unwrap();
        std::thread::sleep(Duration::from_millis(4));
    }
}

fn map_key(k: K) -> Option<Key> {
    Some(match k {
        K::Up => Key::Up,
        K::Down => Key::Down,
        K::Left => Key::Left,
        K::Right => Key::Right,
        K::Space | K::Enter => Key::Select,
        K::M | K::Escape => Key::Menu,
        _ => return None,
    })
}

pub fn write_png(path: &Path, cv: &Canvas) {
    let file = std::fs::File::create(path).unwrap();
    let mut enc = png::Encoder::new(std::io::BufWriter::new(file), cv.w as u32, cv.h as u32);
    enc.set_color(png::ColorType::Rgb);
    enc.set_depth(png::BitDepth::Eight);
    let mut wr = enc.write_header().unwrap();
    // Exactly what the panel shows: dithered RGB565.
    let data: Vec<u8> = cv.rgb565(cv.bounds()).flat_map(|p| {
        let c = from565(p);
        [(c >> 16) as u8, (c >> 8) as u8, c as u8]
    }).collect();
    wr.write_image_data(&data).unwrap();
}
