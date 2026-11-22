//! The simulator's HTTP server: a thin wrapper around `reader_ui::web`, the
//! same routes the device serves over Wi-Fi.

use std::io::{BufRead, BufReader, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::{Arc, Mutex};
use std::time::Instant;

use std::path::PathBuf;

use reader_core::App;
use reader_ui::web::{book_piece, respond, upload_limit, url_decode, Body};

/// `start` is the simulator's clock origin, so web actions share its timeline.
pub fn serve(app: Arc<Mutex<App>>, books: Arc<Mutex<Vec<PathBuf>>>, addr: &str, start: Instant) {
    let listener = match TcpListener::bind(addr) {
        Ok(l) => l,
        Err(e) => {
            eprintln!("web view disabled: {e}");
            return;
        }
    };
    std::thread::spawn(move || {
        for stream in listener.incoming().flatten() {
            let (app, books) = (app.clone(), books.clone());
            std::thread::spawn(move || {
                let _ = handle(stream, &app, &books, start);
            });
        }
    });
}

fn handle(mut stream: TcpStream, app: &Mutex<App>, books: &Mutex<Vec<PathBuf>>, start: Instant) -> std::io::Result<()> {
    let mut reader = BufReader::new(stream.try_clone()?);
    let mut line = String::new();
    reader.read_line(&mut line)?;
    let mut parts = line.split_whitespace();
    let method = parts.next().unwrap_or("").to_string();
    let target = parts.next().unwrap_or("/").to_string();
    let now = start.elapsed().as_millis() as u64;
    let mut length = 0usize;
    loop {
        let mut h = String::new();
        if reader.read_line(&mut h)? == 0 || h == "\r\n" {
            break;
        }
        if let Some(v) = h.to_ascii_lowercase().strip_prefix("content-length:") {
            length = v.trim().parse().unwrap_or(0);
        }
    }
    let (path, query) = target.split_once('?').unwrap_or((&target, ""));
    if method == "POST" && path == "/upload" {
        // Same behaviour as the device: store, add to the library, toast.
        let title = query.split('&').find_map(|kv| kv.strip_prefix("title=")).map(url_decode).unwrap_or_default();
        let limit = upload_limit(&app.lock().unwrap());
        if title.is_empty() || length == 0 || length > limit {
            return write!(stream, "HTTP/1.1 400 Bad Request\r\nConnection: close\r\n\r\nneed a title and a book of at most {} MB", limit >> 20);
        }
        let mut body = vec![0u8; length];
        std::io::Read::read_exact(&mut reader, &mut body)?;
        // With a card, books go to the card (as on the device).
        let on_card = app.lock().unwrap().card.is_some();
        let dir = if on_card { format!("{}/BOOKS", crate::SIM_CARD) } else { crate::UPLOADS.into() };
        std::fs::create_dir_all(&dir)?;
        let slug: String = title.chars().map(|c| if c.is_alphanumeric() { c.to_ascii_lowercase() } else { '-' }).collect();
        let file = PathBuf::from(dir).join(format!("{}.{}", slug.trim_matches('-'), if on_card { "BK" } else { "md" }));
        std::fs::write(&file, &body)?;
        books.lock().unwrap().push(file);
        let place = if on_card { reader_core::app::Place::Card } else { reader_core::app::Place::Flash };
        app.lock().unwrap().book_added(title, reader_core::app::BookInfo { place, bytes: length as u32 }, now);
        return write!(stream, "HTTP/1.1 202 Accepted\r\nConnection: close\r\n\r\nsaving");
    }
    // Deleting, as on the device: needs the page's confirmation, and only
    // uploaded or card books can go (never the simulator's book folders).
    if method == "POST" && (path == "/books/delete" || path == "/books/clear") {
        let param = |name: &str| query.split('&').find_map(|kv| kv.strip_prefix(name)?.strip_prefix('='));
        let deletable = |p: &PathBuf| p.starts_with(crate::UPLOADS) || p.starts_with(crate::SIM_CARD);
        let mut list = books.lock().unwrap();
        let mut a = app.lock().unwrap();
        let victims: Vec<usize> = if path == "/books/delete" {
            match (param("i").and_then(|v| v.parse::<usize>().ok()), param("confirm")) {
                (Some(i), Some("yes")) if list.get(i).is_some_and(deletable) => vec![i],
                (Some(_), Some("yes")) => return write!(stream, "HTTP/1.1 403 Forbidden\r\nConnection: close\r\n\r\nthis book can't be deleted"),
                _ => return write!(stream, "HTTP/1.1 400 Bad Request\r\nConnection: close\r\n\r\nnot confirmed"),
            }
        } else {
            let dir = match param("place") {
                Some("card") => crate::SIM_CARD,
                Some("memory") => crate::UPLOADS,
                _ => return write!(stream, "HTTP/1.1 400 Bad Request\r\nConnection: close\r\n\r\nwhich place?"),
            };
            if param("confirm") != Some("DELETE") {
                return write!(stream, "HTTP/1.1 400 Bad Request\r\nConnection: close\r\n\r\nnot confirmed");
            }
            (0..list.len()).filter(|&i| list[i].starts_with(dir)).collect()
        };
        for &i in victims.iter().rev() {
            let _ = std::fs::remove_file(&list[i]);
            list.remove(i);
            a.book_removed(i, now);
        }
        return write!(stream, "HTTP/1.1 202 Accepted\r\nConnection: close\r\n\r\ndeleted {}", victims.len());
    }
    let (method, target) = (method.as_str(), target.as_str());
    let r = respond(&mut app.lock().unwrap(), method, target, now);
    let extra = r.extra_header.map(|h| format!("{h}\r\n")).unwrap_or_default();
    write!(stream, "HTTP/1.1 {}\r\nContent-Type: {}\r\nCache-Control: no-store\r\nConnection: close\r\n{extra}\r\n", r.status, r.ctype)?;
    match r.body {
        Body::Static(s) => stream.write_all(s.as_bytes()),
        Body::Text(s) => stream.write_all(s.as_bytes()),
        Body::Bytes(b) => stream.write_all(&b),
        Body::Raw(b) => stream.write_all(b),
        Body::Book { from, to } => {
            // In pieces, releasing the lock between them (as the device must).
            let mut at = from;
            while at < to {
                let mut piece = String::new();
                at = book_piece(&app.lock().unwrap(), at, to, &mut piece);
                stream.write_all(piece.as_bytes())?;
            }
            Ok(())
        }
    }
}
