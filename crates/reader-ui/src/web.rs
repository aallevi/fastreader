//! The web view's HTTP routes, shared by the simulator and the device, so a
//! phone sees exactly the same thing whichever one serves it.
//!
//!   GET  /                 the page (web/index.html)
//!   GET  /book?from=N&count=M   whole paragraphs around words N..N+M as HTML;
//!                          the `X-Range: a-b` header says which words came
//!   GET  /state            {"pos","len","wpm","playing","permille","theme","book","mode","title"}
//!   GET  /theme.css        CSS variables from the active theme
//!   GET  /settings         every setting with its range, plus option names
//!   POST /set?name=wpm&value=450   change one setting (validated by the core)
//!   POST /seek?w=N         jump to word N
//!   POST /key?k=up         press a key (up/down/left/right/select/menu)
//!
//! Book HTML is produced in pieces ([`Body::Book`]) so the device can stream
//! a 170k-word book without ever holding it in memory as HTML.

use alloc::format;
use alloc::string::{String, ToString};
use alloc::vec::Vec;
use core::fmt::Write;

use reader_core::app::{PickKind, BRIGHTNESS_NAMES, SIZE_NAMES};
use reader_core::settings::{ALIGN_NAMES, FIELDS, MODE_NAMES, PARAGRAPH_NAMES, SKIP_NAMES, SPACING_NAMES};
use reader_core::{App, Key, Screen};

use crate::themes::theme;

pub const PAGE: &str = include_str!("../../../web/index.html");

/// How many words of book HTML to produce per piece when streaming.
pub const BOOK_PIECE: usize = 400;

pub enum Body {
    Static(&'static str),
    Text(String),
    /// Book words `from..to` as HTML; write it with [`book_piece`] in steps.
    Book { from: usize, to: usize },
}

pub struct Response {
    pub status: &'static str,
    pub ctype: &'static str,
    pub extra_header: Option<String>,
    pub body: Body,
}

impl Response {
    fn ok(ctype: &'static str, body: Body) -> Self {
        Response { status: "200 OK", ctype, extra_header: None, body }
    }
    fn text(status: &'static str, msg: &str) -> Self {
        Response { status, ctype: "text/plain; charset=utf-8", extra_header: None, body: Body::Text(msg.to_string()) }
    }
}

/// Handles one request line (`GET /state HTTP/1.1`'s method and target).
pub fn respond(app: &mut App, method: &str, target: &str, now: u64) -> Response {
    let (path, query) = target.split_once('?').unwrap_or((target, ""));
    let param = |name: &str| query.split('&').find_map(|kv| kv.strip_prefix(name)?.strip_prefix('='));
    let num = |name: &str| param(name).and_then(|v| v.parse::<usize>().ok());
    match (method, path) {
        ("GET", "/") | ("GET", "/index.html") => Response::ok("text/html; charset=utf-8", Body::Static(PAGE)),
        ("GET", "/book") => {
            let n = app.reader.book.len();
            let from = num("from").unwrap_or(0).min(n);
            let count = num("count").unwrap_or(2000).clamp(1, 20_000);
            let r = app.reader.book.paragraphs_around(from, from + count);
            Response {
                status: "200 OK",
                ctype: "text/html; charset=utf-8",
                extra_header: Some(format!("X-Range: {}-{}", r.start, r.end)),
                body: Body::Book { from: r.start, to: r.end },
            }
        }
        ("GET", "/state") => Response::ok("application/json", Body::Text(state_json(app))),
        ("GET", "/theme.css") => Response::ok("text/css", Body::Text(theme(app.settings.theme).css_vars())),
        ("GET", "/settings") => Response::ok("application/json", Body::Text(settings_json(app))),
        ("POST", "/set") => {
            let name = param("name").unwrap_or("");
            let Some(v) = param("value").and_then(|v| v.parse::<u16>().ok()) else {
                return Response::text("400 Bad Request", "need ?name=..&value=N");
            };
            let r = if name == "book" {
                // Changing book goes through the same path as the menu.
                app.screen = Screen::Pick { kind: PickKind::Book, sel: v as u8, original: app.settings.book };
                app.handle(Key::Select, now);
                Ok(())
            } else {
                app.set_setting(name, v)
            };
            match r {
                Ok(()) => Response::ok("application/json", Body::Text(settings_json(app))),
                Err(e) => Response::text("400 Bad Request", &e),
            }
        }
        ("POST", "/seek") => match num("w") {
            Some(w) => {
                app.reader.seek(w, now);
                Response::text("204 No Content", "")
            }
            None => Response::text("400 Bad Request", "need ?w=N"),
        },
        ("POST", "/key") => {
            let key = match param("k") {
                Some("up") => Key::Up,
                Some("down") => Key::Down,
                Some("left") => Key::Left,
                Some("right") => Key::Right,
                Some("select") => Key::Select,
                Some("menu") => Key::Menu,
                _ => return Response::text("400 Bad Request", "unknown key"),
            };
            app.handle(key, now);
            Response::text("204 No Content", "")
        }
        _ => Response::text("404 Not Found", "not found"),
    }
}

/// Appends book HTML for words `from..min(from + BOOK_PIECE, to)`; returns
/// where the next piece starts (== `to` when finished). Pieces end on
/// paragraph boundaries so the HTML stays well formed.
pub fn book_piece(app: &App, from: usize, to: usize, out: &mut String) -> usize {
    let book = &app.reader.book;
    if from >= to {
        return to;
    }
    let end = book.paragraphs_around(from, (from + BOOK_PIECE).min(to)).end.min(to).max(from + 1);
    let _ = book.write_html_range(from..end, out);
    end
}

fn json_str(s: &str) -> String {
    let mut out = String::from("\"");
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            c if (c as u32) < 0x20 => {
                let _ = write!(out, "\\u{:04x}", c as u32);
            }
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

fn json_list<'a>(items: impl Iterator<Item = &'a str>) -> String {
    let v: Vec<String> = items.map(json_str).collect();
    format!("[{}]", v.join(","))
}

pub fn state_json(a: &App) -> String {
    format!(
        "{{\"pos\":{},\"len\":{},\"wpm\":{},\"playing\":{},\"permille\":{},\"theme\":{},\"book\":{},\"mode\":{},\"title\":{}}}",
        a.reader.pos(),
        a.reader.book.len(),
        a.reader.wpm(),
        a.reader.is_playing(),
        a.reader.permille(),
        a.settings.theme,
        a.settings.book,
        a.settings.mode,
        json_str(&a.reader.book.title)
    )
}

/// Current values with their ranges, plus the names for every choice list.
pub fn settings_json(a: &App) -> String {
    let fields: Vec<String> = FIELDS
        .iter()
        .map(|(n, lo, hi)| format!("{}:{{\"value\":{},\"min\":{lo},\"max\":{hi}}}", json_str(n), a.settings.get(n).unwrap_or(0)))
        .collect();
    format!(
        "{{\"fields\":{{{}}},\"themes\":{},\"fonts\":{},\"skips\":{},\"books\":{},\"sizes\":{},\"brightness\":{},\"modes\":{},\"spacings\":{},\"paragraphs\":{},\"aligns\":{}}}",
        fields.join(","),
        json_list(a.catalog.themes.iter().copied()),
        json_list(a.catalog.fonts.iter().copied()),
        json_list(SKIP_NAMES.iter().copied()),
        json_list(a.library.iter().map(|s| s.as_str())),
        json_list(SIZE_NAMES.iter().copied()),
        json_list(BRIGHTNESS_NAMES.iter().copied()),
        json_list(MODE_NAMES.iter().copied()),
        json_list(SPACING_NAMES.iter().copied()),
        json_list(PARAGRAPH_NAMES.iter().copied()),
        json_list(ALIGN_NAMES.iter().copied()),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use reader_core::{Book, Settings};

    fn app() -> App {
        let text = "# One\nAlpha \"beta\". Gamma.\n\nDelta epsilon.\n\n# Two\nZeta.";
        App::new(Book::new("Tést \"q\"", text), crate::CATALOG, Settings::default())
    }

    #[test]
    fn json_is_escaped() {
        let s = state_json(&app());
        assert!(s.contains(r#""title":"Tést \"q\"""#), "{s}");
    }

    #[test]
    fn book_streams_in_pieces_identical_to_whole() {
        let a = app();
        let mut whole = String::new();
        a.reader.book.write_html(&mut whole).unwrap();
        let mut pieces = String::new();
        let mut at = 0;
        while at < a.reader.book.len() {
            at = book_piece(&a, at, a.reader.book.len(), &mut pieces);
        }
        assert_eq!(pieces, whole);
    }

    #[test]
    fn routes_answer_and_refuse() {
        let mut a = app();
        assert_eq!(respond(&mut a, "GET", "/", 0).status, "200 OK");
        let r = respond(&mut a, "GET", "/book?from=3&count=2", 0);
        assert_eq!(r.extra_header.as_deref(), Some("X-Range: 1-6"), "whole paragraphs around words 3..5");
        assert_eq!(respond(&mut a, "POST", "/set?name=wpm&value=9999", 0).status, "200 OK");
        assert_eq!(a.reader.wpm(), 1000);
        assert_eq!(respond(&mut a, "POST", "/set?name=nope&value=1", 0).status, "400 Bad Request");
        assert_eq!(respond(&mut a, "POST", "/key?k=select", 0).status, "204 No Content");
        assert!(a.reader.is_playing());
        assert_eq!(respond(&mut a, "GET", "/nothing", 0).status, "404 Not Found");
    }
}
