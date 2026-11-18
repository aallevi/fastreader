//! Tiny HTTP server for the web view. The firmware serves the same page and
//! routes over Wi-Fi; this one runs on localhost for the simulator.
//!
//!   GET  /            the page (web/index.html)
//!   GET  /book        the whole book as styled HTML, one <span> per word
//!   GET  /state       {"pos","wpm","playing","permille","theme","title"}
//!   GET  /theme.css   CSS variables from the active theme
//!   POST /seek?w=N    jump to word N
//!   POST /key?k=up    press a key (up/down/left/right/select/menu)
//!   GET  /settings    every setting, its range, and the option names
//!   POST /set?name=wpm&value=450   change one setting (validated by the core)

use std::io::{BufRead, BufReader, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::{Arc, Mutex};
use std::time::Instant;

use reader_core::{App, Key};

const PAGE: &str = include_str!("../../../web/index.html");

/// `start` is the simulator's clock origin, so web actions share its timeline.
pub fn serve(app: Arc<Mutex<App>>, addr: &str, start: Instant) {
    let listener = match TcpListener::bind(addr) {
        Ok(l) => l,
        Err(e) => {
            eprintln!("web view disabled: {e}");
            return;
        }
    };
    std::thread::spawn(move || {
        for stream in listener.incoming().flatten() {
            let app = app.clone();
            std::thread::spawn(move || {
                let _ = handle(stream, &app, start);
            });
        }
    });
}

fn handle(mut stream: TcpStream, app: &Mutex<App>, start: Instant) -> std::io::Result<()> {
    let mut line = String::new();
    BufReader::new(&stream).read_line(&mut line)?;
    let mut parts = line.split_whitespace();
    let method = parts.next().unwrap_or("");
    let target = parts.next().unwrap_or("/");
    let (path, query) = target.split_once('?').unwrap_or((target, ""));
    let param = |name: &str| query.split('&').find_map(|kv| kv.strip_prefix(name)?.strip_prefix('='));
    let now = start.elapsed().as_millis() as u64;

    let (status, ctype, body) = match (method, path) {
        ("GET", "/") => ("200 OK", "text/html; charset=utf-8", PAGE.to_string()),
        ("GET", "/book") => {
            let mut s = String::new();
            app.lock().unwrap().reader.book.write_html(&mut s).unwrap();
            ("200 OK", "text/html; charset=utf-8", s)
        }
        ("GET", "/state") => {
            let a = app.lock().unwrap();
            let body = format!(
                "{{\"pos\":{},\"wpm\":{},\"playing\":{},\"permille\":{},\"theme\":{},\"book\":{},\"title\":{:?}}}",
                a.reader.pos(),
                a.reader.wpm(),
                a.reader.is_playing(),
                a.reader.permille(),
                a.settings.theme,
                a.settings.book,
                a.reader.book.title
            );
            ("200 OK", "application/json", body)
        }
        ("GET", "/theme.css") => {
            let a = app.lock().unwrap();
            ("200 OK", "text/css", reader_ui::themes::theme(a.settings.theme).css_vars())
        }
        ("GET", "/settings") => {
            let a = app.lock().unwrap();
            ("200 OK", "application/json", settings_json(&a))
        }
        ("POST", "/set") => {
            let name = param("name").unwrap_or("");
            match param("value").and_then(|v| v.parse::<u16>().ok()) {
                Some(v) => {
                    let mut a = app.lock().unwrap();
                    // Changing book goes through the same path as the menu.
                    let r = if name == "book" {
                        a.screen = reader_core::Screen::Pick { kind: reader_core::app::PickKind::Book, sel: v as u8, original: a.settings.book };
                        a.handle(Key::Select, now);
                        Ok(())
                    } else {
                        a.set_setting(name, v)
                    };
                    match r {
                        Ok(()) => ("200 OK", "application/json", settings_json(&a)),
                        Err(e) => ("400 Bad Request", "text/plain", e),
                    }
                }
                None => ("400 Bad Request", "text/plain", "need ?name=..&value=N".into()),
            }
        }
        ("POST", "/seek") => match param("w").and_then(|w| w.parse::<usize>().ok()) {
            Some(w) => {
                app.lock().unwrap().reader.seek(w, now);
                ("204 No Content", "text/plain", String::new())
            }
            None => ("400 Bad Request", "text/plain", "need ?w=N".into()),
        },
        ("POST", "/key") => {
            let key = match param("k") {
                Some("up") => Some(Key::Up),
                Some("down") => Some(Key::Down),
                Some("left") => Some(Key::Left),
                Some("right") => Some(Key::Right),
                Some("select") => Some(Key::Select),
                Some("menu") => Some(Key::Menu),
                _ => None,
            };
            match key {
                Some(k) => {
                    app.lock().unwrap().handle(k, now);
                    ("204 No Content", "text/plain", String::new())
                }
                None => ("400 Bad Request", "text/plain", "unknown key".into()),
            }
        }
        _ => ("404 Not Found", "text/plain", "not found".into()),
    };
    write!(
        stream,
        "HTTP/1.1 {status}\r\nContent-Type: {ctype}\r\nContent-Length: {}\r\nCache-Control: no-store\r\nConnection: close\r\n\r\n{body}",
        body.len()
    )
}

fn json_list<'a>(items: impl Iterator<Item = &'a str>) -> String {
    let v: Vec<String> = items.map(|s| format!("{s:?}")).collect();
    format!("[{}]", v.join(","))
}

/// Current values with their ranges, plus the names for every choice list.
fn settings_json(a: &App) -> String {
    use reader_core::settings::{FIELDS, SKIP_NAMES};
    let fields: Vec<String> = FIELDS
        .iter()
        .map(|(n, lo, hi)| format!("{n:?}:{{\"value\":{},\"min\":{lo},\"max\":{hi}}}", a.settings.get(n).unwrap_or(0)))
        .collect();
    format!(
        "{{\"fields\":{{{}}},\"themes\":{},\"fonts\":{},\"skips\":{},\"books\":{},\"sizes\":{},\"brightness\":{}}}",
        fields.join(","),
        json_list(a.catalog.themes.iter().copied()),
        json_list(a.catalog.fonts.iter().copied()),
        json_list(SKIP_NAMES.iter().copied()),
        json_list(a.library.iter().map(|s| s.as_str())),
        json_list(reader_core::app::SIZE_NAMES.iter().copied()),
        json_list(reader_core::app::BRIGHTNESS_NAMES.iter().copied()),
    )
}
