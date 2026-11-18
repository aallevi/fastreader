//! The simulator's HTTP server: a thin wrapper around `reader_ui::web`, the
//! same routes the device serves over Wi-Fi.

use std::io::{BufRead, BufReader, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::{Arc, Mutex};
use std::time::Instant;

use reader_core::App;
use reader_ui::web::{book_piece, respond, Body};

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
    let now = start.elapsed().as_millis() as u64;
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
