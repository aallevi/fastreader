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
use reader_core::settings::{ALIGN_NAMES, FIELDS, MODE_NAMES, BRACKET_NAMES, POSITION_NAMES, PARAGRAPH_NAMES, SKIP_NAMES, SPACING_NAMES};
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
    Bytes(Vec<u8>),
    /// Bytes built into the program (pdf.js, gzip-compressed).
    Raw(&'static [u8]),
}

/// pdf.js, for turning PDFs into books on the phone (it has no internet on
/// the reader's Wi-Fi). Apache-2.0, see assets/web/.
pub const PDFJS: &[u8] = include_bytes!("../../../assets/web/pdf.min.mjs.gz");
pub const PDFJS_WORKER: &[u8] = include_bytes!("../../../assets/web/pdf.worker.min.mjs.gz");

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
        // The theme's colours go inside the page: a phone that has just joined
        // fires many requests at once, and a lost /theme.css left the page
        // without colours.
        ("GET", "/") | ("GET", "/index.html") => {
            let t = app.settings.theme;
            let page = PAGE.replacen("/*THEME*/", &theme(t).css_vars(), 1).replacen("/*THEME_ID*/", &alloc::format!("{t}"), 1);
            Response::ok("text/html; charset=utf-8", Body::Text(page))
        }
        // RFC 8908 captive-portal API (advertised through DHCP option 114).
        ("GET", "/captive-portal-api") => Response::ok(
            "application/captive+json",
            Body::Static("{\"captive\":true,\"user-portal-url\":\"http://192.168.4.1/\"}"),
        ),
        // Phones and laptops probe these on a new Wi-Fi; answering with a
        // redirect makes them pop the reader page up by themselves.
        ("GET", p) if CAPTIVE_PROBES.contains(&p) => Response {
            status: "302 Found",
            ctype: "text/html; charset=utf-8",
            extra_header: Some("Location: http://192.168.4.1/".into()),
            body: Body::Static("<a href=\"http://192.168.4.1/\">Fast Reader</a>"),
        },
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
        ("GET", p) if p.starts_with("/img/") => match p[5..].parse::<usize>().ok().and_then(|n| app.reader.book.images.get(n)) {
            Some(img) => Response::ok("image/bmp", Body::Bytes(bmp(&img))),
            None => Response::text("404 Not Found", "no such image"),
        },
        ("GET", "/pdfjs/pdf.min.mjs") | ("GET", "/pdfjs/pdf.worker.min.mjs") => Response {
            status: "200 OK",
            ctype: "text/javascript",
            extra_header: Some("Content-Encoding: gzip".into()),
            body: Body::Raw(if path.contains("worker") { PDFJS_WORKER } else { PDFJS }),
        },
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

/// A picture as a 16-bit BMP (RGB565 bit fields): no compression code needed.
pub fn bmp(img: &reader_core::book::Image) -> Vec<u8> {
    let (w, h) = (img.w as u32, img.h as u32);
    let row = (w * 2).div_ceil(4) * 4;
    let data = row * h;
    let offset = 14 + 40 + 12;
    let mut b = Vec::with_capacity((offset + data) as usize);
    b.extend_from_slice(b"BM");
    for v in [offset + data, 0, offset] {
        b.extend_from_slice(&v.to_le_bytes());
    }
    for v in [40u32, w, (-(h as i32)) as u32] {
        b.extend_from_slice(&v.to_le_bytes()); // negative height: rows top to bottom
    }
    b.extend_from_slice(&1u16.to_le_bytes());
    b.extend_from_slice(&16u16.to_le_bytes());
    for v in [3u32, data, 2835, 2835, 0, 0, 0xF800, 0x07E0, 0x001F] {
        b.extend_from_slice(&v.to_le_bytes()); // BI_BITFIELDS + RGB565 masks
    }
    for y in 0..h as usize {
        for x in 0..w as usize {
            b.extend_from_slice(&img.px[y * w as usize + x].to_le_bytes());
        }
        b.resize(b.len() + (row - w * 2) as usize, 0);
    }
    b
}

/// Most an uploaded book may weigh (it is held in RAM before being saved).
pub const MAX_UPLOAD: usize = 3 * 1024 * 1024;
/// With an SD card, books go to the card and may be this big (a scanned art
/// book with all its pictures is ~60 MB).
pub const MAX_CARD_UPLOAD: usize = 256 * 1024 * 1024;

/// The upload limit right now: card or the reader's own flash.
pub fn upload_limit(a: &App) -> usize {
    match a.card {
        Some(c) if c.formatted => MAX_CARD_UPLOAD.min((c.free_mb as usize).saturating_sub(16) << 20),
        Some(_) => MAX_UPLOAD,
        None => MAX_UPLOAD,
    }
}

/// Decodes a query parameter (`%XX` and `+`).
pub fn url_decode(s: &str) -> String {
    let b = s.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        match b[i] {
            b'+' => out.push(b' '),
            b'%' if i + 2 < b.len() => {
                let hex = |c: u8| (c as char).to_digit(16);
                match (hex(b[i + 1]), hex(b[i + 2])) {
                    (Some(h), Some(l)) => {
                        out.push((h * 16 + l) as u8);
                        i += 2;
                    }
                    _ => out.push(b'%'),
                }
            }
            c => out.push(c),
        }
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// Connectivity-check URLs of Android, Apple, Windows, Firefox and others.
const CAPTIVE_PROBES: [&str; 9] = [
    "/generate_204",
    "/gen_204",
    "/hotspot-detect.html",
    "/library/test/success.html",
    "/connecttest.txt",
    "/ncsi.txt",
    "/redirect",
    "/canonical.html",
    "/success.txt",
];

/// Answers a DNS query with `ip` for every name (A records), so any address
/// typed on the reader's Wi-Fi leads to the reader. Returns the reply length.
pub fn captive_dns_reply(query: &[u8], ip: [u8; 4], out: &mut [u8]) -> Option<usize> {
    if query.len() < 12 || query[2] & 0x80 != 0 || u16::from_be_bytes([query[4], query[5]]) == 0 {
        return None; // not a query, or no question
    }
    // Walk the first question's name to find its type.
    let mut i = 12;
    while i < query.len() && query[i] != 0 {
        if query[i] & 0xC0 != 0 {
            return None; // compression in a question: not worth supporting
        }
        i += query[i] as usize + 1;
    }
    let end = i + 5; // zero byte + type + class
    if end > query.len() || end + 16 > out.len() {
        return None;
    }
    let qtype = u16::from_be_bytes([query[i + 1], query[i + 2]]);
    let answer = qtype == 1 || qtype == 255;
    out[..end].copy_from_slice(&query[..end]);
    out[2] = 0x81 | (query[2] & 0x01); // response, keep "recursion desired"
    out[3] = 0x80; // recursion available, no error
    out[4..6].copy_from_slice(&1u16.to_be_bytes());
    out[6..8].copy_from_slice(&(answer as u16).to_be_bytes());
    out[8..12].fill(0);
    if !answer {
        return Some(end);
    }
    let rr = [0xC0, 0x0C, 0, 1, 0, 1, 0, 0, 0, 60, 0, 4, ip[0], ip[1], ip[2], ip[3]];
    out[end..end + 16].copy_from_slice(&rr);
    Some(end + 16)
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
        "{{\"pos\":{},\"len\":{},\"wpm\":{},\"playing\":{},\"permille\":{},\"theme\":{},\"book\":{},\"mode\":{},\"library\":{},\"bookmarks\":{:?},\"title\":{}}}",
        a.reader.pos(),
        a.reader.book.len(),
        a.reader.wpm(),
        a.reader.is_playing(),
        a.reader.permille(),
        a.settings.theme,
        a.settings.book,
        a.settings.mode,
        a.library.len(),
        a.book_marks(),
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
        "{{\"fields\":{{{}}},\"themes\":{},\"fonts\":{},\"skips\":{},\"books\":{},\"sizes\":{},\"brightness\":{},\"modes\":{},\"spacings\":{},\"paragraphs\":{},\"aligns\":{},\"brackets\":{},\"positions\":{},\"pictures\":{},\"upload_limit\":{},\"card\":{}}}",
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
        json_list(BRACKET_NAMES.iter().copied()),
        json_list(POSITION_NAMES.iter().copied()),
        json_list(reader_core::settings::PICTURE_NAMES.iter().copied()),
        upload_limit(a),
        a.card.is_some_and(|c| c.formatted),
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
    fn the_page_carries_its_theme() {
        let mut a = App::new(reader_core::Book::new("t", "x"), crate::CATALOG, Settings { theme: 2, ..Settings::default() });
        let Body::Text(page) = respond(&mut a, "GET", "/", 0).body else { panic!() };
        assert!(!page.contains("/*THEME*/") && !page.contains("/*THEME_ID*/"));
        assert!(page.contains(&crate::themes::theme(2).css_vars()), "colours inline, no second request needed");
        assert!(page.contains("data-id=\"2\""));
    }

    #[test]
    fn pdfjs_is_served_compressed() {
        let mut a = App::new(reader_core::Book::new("t", "x"), crate::CATALOG, Settings::default());
        for p in ["/pdfjs/pdf.min.mjs", "/pdfjs/pdf.worker.min.mjs"] {
            let r = respond(&mut a, "GET", p, 0);
            assert_eq!((r.status, r.ctype), ("200 OK", "text/javascript"));
            assert_eq!(r.extra_header.as_deref(), Some("Content-Encoding: gzip"));
            let Body::Raw(b) = r.body else { panic!() };
            assert_eq!(&b[..2], &[0x1f, 0x8b], "gzip data");
        }
    }

    #[test]
    fn images_are_served_as_bmp() {
        let mut file = b"A picture:\n\n{image:0}".to_vec();
        file.extend_from_slice(reader_core::book::IMAGES_MAGIC);
        file.extend_from_slice(&[1, 0, 3, 0, 2, 0]);
        file.extend_from_slice(&[0x00, 0xF8, 0xE0, 0x07, 0x1F, 0x00, 0xFF, 0xFF, 0, 0, 0x00, 0xF8]);
        let mut a = App::new(reader_core::Book::from_bytes(&file, "t"), crate::CATALOG, Settings::default());
        let r = respond(&mut a, "GET", "/img/0", 0);
        assert_eq!(r.ctype, "image/bmp");
        let Body::Bytes(b) = r.body else { panic!() };
        assert_eq!(&b[..2], b"BM");
        assert_eq!(b.len(), 66 + 8 * 2, "3 px rows padded to 8 bytes, 2 rows");
        assert_eq!(respond(&mut a, "GET", "/img/9", 0).status, "404 Not Found");
    }

    #[test]
    fn url_decoding() {
        assert_eq!(url_decode("Differenza+e%20ripetizione"), "Differenza e ripetizione");
        assert_eq!(url_decode("caf%C3%A9"), "café");
        assert_eq!(url_decode("100%"), "100%");
        assert_eq!(url_decode("%zz"), "%zz");
    }

    #[test]
    fn dns_answers_every_name_with_the_reader() {
        // Query for example.com, type A, id 0x1234, recursion desired.
        let mut q = alloc::vec![0x12, 0x34, 0x01, 0x00, 0, 1, 0, 0, 0, 0, 0, 0];
        for label in ["example", "com"] {
            q.push(label.len() as u8);
            q.extend_from_slice(label.as_bytes());
        }
        q.extend_from_slice(&[0, 0, 1, 0, 1]);
        let mut out = [0u8; 512];
        let n = captive_dns_reply(&q, [192, 168, 4, 1], &mut out).unwrap();
        assert_eq!(&out[..2], &[0x12, 0x34], "same id");
        assert_eq!(out[2] & 0x80, 0x80, "is a response");
        assert_eq!(u16::from_be_bytes([out[6], out[7]]), 1, "one answer");
        assert_eq!(&out[n - 4..n], &[192, 168, 4, 1]);
        // AAAA: answered with no records, so phones fall back to IPv4.
        let len = q.len();
        q[len - 3] = 28;
        let n = captive_dns_reply(&q, [192, 168, 4, 1], &mut out).unwrap();
        assert_eq!(u16::from_be_bytes([out[6], out[7]]), 0);
        assert_eq!(n, q.len());
        assert!(captive_dns_reply(&[0u8; 5], [0; 4], &mut out).is_none(), "garbage is ignored");
    }

    #[test]
    fn connectivity_checks_redirect_to_the_reader() {
        let mut a = app();
        assert!(matches!(respond(&mut a, "GET", "/captive-portal-api", 0).body, Body::Static(s) if s.contains("\"captive\":true")));
        for probe in ["/generate_204", "/hotspot-detect.html", "/connecttest.txt"] {
            let r = respond(&mut a, "GET", probe, 0);
            assert_eq!(r.status, "302 Found", "{probe}");
            assert_eq!(r.extra_header.as_deref(), Some("Location: http://192.168.4.1/"));
        }
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
