//! The simulator's HTTP server: a thin wrapper around `reader_ui::web`, the
//! same routes the device serves over Wi-Fi.

use std::io::{BufRead, BufReader, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::{Arc, Mutex};
use std::time::Instant;

use std::path::PathBuf;

use reader_core::App;
use reader_ui::web::{book_piece, respond, url_decode, Body, MAX_UPLOAD};

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
        if title.is_empty() || length == 0 || length > MAX_UPLOAD {
            return write!(stream, "HTTP/1.1 400 Bad Request\r\nConnection: close\r\n\r\nneed a title and a book of at most 3 MB");
        }
        let mut body = vec![0u8; length];
        std::io::Read::read_exact(&mut reader, &mut body)?;
        std::fs::create_dir_all(crate::UPLOADS)?;
        let slug: String = title.chars().map(|c| if c.is_alphanumeric() { c.to_ascii_lowercase() } else { '-' }).collect();
        let file = PathBuf::from(crate::UPLOADS).join(format!("{}.md", slug.trim_matches('-')));
        std::fs::write(&file, &body)?;
        books.lock().unwrap().push(file);
        app.lock().unwrap().book_added(title, now);
        return write!(stream, "HTTP/1.1 202 Accepted\r\nConnection: close\r\n\r\nsaving");
    }
    if method == "POST" && path == "/books/clear" {
        let _ = std::fs::remove_dir_all(crate::UPLOADS);
        let mut list = books.lock().unwrap();
        list.retain(|p| !p.starts_with(crate::UPLOADS));
        app.lock().unwrap().books_removed(list.len(), now);
        return write!(stream, "HTTP/1.1 202 Accepted\r\nConnection: close\r\n\r\nremoving");
    }
    let (method, target) = (method.as_str(), target.as_str());
    let r = respond(&mut app.lock().unwrap(), method, target, now);
    let extra = r.extra_header.map(|h| format!("{h}\r\n")).unwrap_or_default();
    write!(stream, "HTTP/1.1 {}\r\nContent-Type: {}\r\nCache-Control: no-store\r\nConnection: close\r\n{extra}\r\n", r.status, r.ctype)?;
    match r.body {
        Body::Static(s) => stream.write_all(s.as_bytes()),
        Body::Text(s) => stream.write_all(s.as_bytes()),
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
