//! The Wi-Fi web view: an access point called "FastReader", a DHCP server so
//! phones get an address, and three HTTP workers that serve the same routes
//! as the simulator (`reader_ui::web`). Started on demand from the menu.

use core::cell::RefCell;
use core::fmt::Write as _;
use core::net::{Ipv4Addr, SocketAddrV4};

use alloc::string::String;
use embassy_executor::Spawner;
use embassy_net::tcp::TcpSocket;
use embassy_net::{IpListenEndpoint, Ipv4Cidr, Runner, Stack, StackResources, StaticConfigV4};
use embassy_time::{Duration, Timer};
use embedded_io_async::Write;
use esp_hal::peripherals::WIFI;
use esp_hal::rng::Rng;
use esp_hal::time::Instant;
use esp_println::println;
use esp_radio::wifi::ap::AccessPointConfig;
use esp_radio::wifi::{Config, ControllerConfig, Interface, WifiController};
use reader_core::App;
use reader_ui::web::{book_piece, respond, upload_limit, url_decode, Body};

use crate::card;

/// Requests the web tasks hand to the screen loop, which owns the flash.
#[derive(Default)]
pub struct Inbox {
    /// A book to store: (title, text).
    pub upload: Option<(String, alloc::vec::Vec<u8>)>,
    /// A book the web task streamed to the SD card.
    pub card_added: Option<card::Stored>,
    /// Delete one library book (index), confirmed on the page.
    pub delete: Option<usize>,
    /// Delete every book in one place, confirmed by typing DELETE.
    pub clear: Option<reader_core::app::Place>,
}

use crate::mk_static;

pub const SSID: &str = "FastReader";
pub const IP: Ipv4Addr = Ipv4Addr::new(192, 168, 4, 1);
/// Shown in Device info.
pub const ADDRESS: &str = "Wi-Fi FastReader → http://192.168.4.1";

const HTTP_WORKERS: usize = 3;

/// Brings up the access point and spawns the network tasks.
pub fn start(spawner: Spawner, wifi: WIFI<'static>, app: &'static RefCell<App>, inbox: &'static RefCell<Inbox>, sd: card::Shared, clock: Instant) {
    println!("wifi: starting (internal heap free {} KB)", esp_alloc::HEAP.free_caps(esp_alloc::MemoryCapability::Internal.into()) / 1024);
    let ap = Config::AccessPoint(AccessPointConfig::default().with_ssid(SSID.try_into().unwrap()));
    let device = Interface::access_point();
    let controller = match WifiController::new(wifi, ControllerConfig::default().with_initial_config(ap)) {
        Ok(c) => c,
        Err(e) => {
            println!("wifi: failed to start: {:?}", e);
            return;
        }
    };
    let config = embassy_net::Config::ipv4_static(StaticConfigV4 { address: Ipv4Cidr::new(IP, 24), gateway: Some(IP), dns_servers: Default::default() });
    let rng = Rng::new();
    let seed = (rng.random() as u64) << 32 | rng.random() as u64;
    // Sockets: one per HTTP worker, plus DHCP.
    let (stack, runner) = embassy_net::new(device, config, mk_static!(StackResources<6>, StackResources::<6>::new()), seed);
    spawner.spawn(connection(controller).unwrap());
    spawner.spawn(net_task(runner).unwrap());
    spawner.spawn(dhcp(stack).unwrap());
    spawner.spawn(dns(stack).unwrap());
    for _ in 0..HTTP_WORKERS {
        spawner.spawn(http(stack, app, inbox, sd, clock).unwrap());
    }
    println!("wifi: access point {} up, http://{}", SSID, IP);
}

#[embassy_executor::task]
async fn connection(controller: WifiController<'static>) {
    loop {
        match controller.wait_for_access_point_connected_event_async().await {
            Ok(esp_radio::wifi::ap::EventInfo::Connected(info)) => println!("wifi: client joined {:?}", info),
            Ok(esp_radio::wifi::ap::EventInfo::Disconnected(info)) => println!("wifi: client left {:?}", info),
            _ => Timer::after(Duration::from_millis(500)).await,
        }
    }
}

#[embassy_executor::task]
async fn net_task(mut runner: Runner<'static, Interface>) {
    runner.run().await
}

/// Hands out addresses to phones and laptops that join the access point.
#[embassy_executor::task]
async fn dhcp(stack: Stack<'static>) {
    use edge_dhcp::io::{self, DEFAULT_SERVER_PORT};
    use edge_dhcp::server::{Server, ServerOptions};
    use edge_nal::UdpBind;
    use edge_nal_embassy::{Udp, UdpBuffers};

    let mut buf = [0u8; 1500];
    let mut gw = [Ipv4Addr::UNSPECIFIED];
    let buffers = UdpBuffers::<1, 1024, 1024, 10>::new();
    let udp = Udp::new(stack, &buffers);
    let Ok(mut socket) = udp.bind(core::net::SocketAddr::V4(SocketAddrV4::new(Ipv4Addr::UNSPECIFIED, DEFAULT_SERVER_PORT))).await else {
        println!("dhcp: could not bind");
        return;
    };
    let dns = [IP];
    loop {
        // Advertise the reader as DNS server (so the captive DNS is used) and
        // the RFC 8910 captive-portal API, which recent phones open directly.
        let mut options = ServerOptions::new(IP, Some(&mut gw));
        options.dns = &dns;
        options.captive_url = Some("http://192.168.4.1/captive-portal-api");
        if let Err(e) = io::server::run(&mut Server::<_, 16>::new_with_et(IP), &options, &mut socket, &mut buf).await {
            println!("dhcp: {:?}", e);
        }
        Timer::after(Duration::from_millis(500)).await;
    }
}

/// Captive portal DNS: every name resolves to the reader, so a phone that
/// joins FastReader opens the reader page by itself.
#[embassy_executor::task]
async fn dns(stack: Stack<'static>) {
    use embassy_net::udp::{PacketMetadata, UdpSocket};
    let mut rx_meta = [PacketMetadata::EMPTY; 4];
    let mut tx_meta = [PacketMetadata::EMPTY; 4];
    let mut rx = [0u8; 1024];
    let mut tx = [0u8; 1024];
    let mut socket = UdpSocket::new(stack, &mut rx_meta, &mut rx, &mut tx_meta, &mut tx);
    if socket.bind(53).is_err() {
        println!("dns: could not bind");
        return;
    }
    let mut query = [0u8; 512];
    let mut reply = [0u8; 512];
    loop {
        let Ok((n, meta)) = socket.recv_from(&mut query).await else { continue };
        if let Some(len) = reader_ui::web::captive_dns_reply(&query[..n], IP.octets(), &mut reply) {
            let _ = socket.send_to(&reply[..len], meta.endpoint).await;
        }
    }
}

/// One HTTP/1.0-style worker: read the request line, answer through the
/// shared handler, close. The app is borrowed only between awaits.
#[embassy_executor::task(pool_size = 3)]
async fn http(stack: Stack<'static>, app: &'static RefCell<App>, inbox: &'static RefCell<Inbox>, sd: card::Shared, clock: Instant) {
    // 4 KB receive window: big uploads (an illustrated book is ~30 MB) go
    // several times faster than with 1 KB.
    // Buffers on the heap, so each worker's task state stays small.
    let mut rx = alloc::vec![0u8; 4096];
    let mut tx = alloc::vec![0u8; 2048];
    let mut head = alloc::vec![0u8; 1024];
    let mut chunk = alloc::vec![0u8; 4096];
    loop {
        let mut socket = TcpSocket::new(stack, &mut rx, &mut tx);
        socket.set_timeout(Some(Duration::from_secs(10)));
        if socket.accept(IpListenEndpoint { addr: None, port: 80 }).await.is_err() {
            continue;
        }
        head.fill(0);
        let mut n = 0;
        while n < head.len() {
            match socket.read(&mut head[n..]).await {
                Ok(0) | Err(_) => break,
                Ok(k) => {
                    n += k;
                    if head[..n].windows(4).any(|w| w == b"\r\n\r\n") {
                        break;
                    }
                }
            }
        }
        let head_text = core::str::from_utf8(&head[..n]).unwrap_or("");
        let line = head_text.lines().next().unwrap_or("");
        let mut parts = line.split_whitespace();
        let (method, target) = (parts.next().unwrap_or(""), parts.next().unwrap_or("/"));
        let now = clock.elapsed().as_millis();
        let (path, query) = target.split_once('?').unwrap_or((target, ""));

        // Uploads carry a body: read it here, let the screen loop store it.
        if method == "POST" && path == "/upload" {
            let length = head_text
                .lines()
                .find_map(|l| l.to_ascii_lowercase().strip_prefix("content-length:").and_then(|v| v.trim().parse::<usize>().ok()))
                .unwrap_or(0);
            let title = query.split('&').find_map(|kv| kv.strip_prefix("title=")).map(url_decode).unwrap_or_default();
            let (limit, to_card) = {
                let a = app.borrow();
                (upload_limit(&a), a.card.is_some_and(|c| c.formatted))
            };
            let start = head_text.find("\r\n\r\n").map_or(n, |p| p + 4);
            let reply: &[u8] = if length == 0 || length > limit || title.is_empty() {
                b"HTTP/1.1 400 Bad Request\r\nContent-Type: text/plain\r\nConnection: close\r\n\r\nneed a title and a book that fits"
            } else if to_card {
                // Straight to the SD card, piece by piece: no size limit from memory.
                let created = sd.borrow_mut().as_mut().and_then(|c| c.create());
                match created {
                    None => b"HTTP/1.1 500 Internal Server Error\r\nContent-Type: text/plain\r\nConnection: close\r\n\r\ncould not write to the SD card",
                    Some((file, name)) => {
                        let mut written = 0usize;
                        let mut ok = true;
                        let first = &head[start..n.min(start + length)];
                        if !first.is_empty() {
                            ok &= sd.borrow_mut().as_mut().is_some_and(|c| c.write(file, first));
                            written += first.len();
                        }
                        while ok && written < length {
                            match socket.read(&mut chunk).await {
                                Ok(0) | Err(_) => break,
                                Ok(k) => {
                                    let k = k.min(length - written);
                                    ok &= sd.borrow_mut().as_mut().is_some_and(|c| c.write(file, &chunk[..k]));
                                    written += k;
                                }
                            }
                        }
                        let done = ok && written == length;
                        if let Some(c) = sd.borrow_mut().as_mut() {
                            c.finish(file, &name, done);
                        }
                        if done {
                            inbox.borrow_mut().card_added = Some(card::Stored { title, file: name, len: length as u32 });
                            b"HTTP/1.1 202 Accepted\r\nContent-Type: text/plain\r\nConnection: close\r\n\r\nsaving"
                        } else {
                            b"HTTP/1.1 400 Bad Request\r\nContent-Type: text/plain\r\nConnection: close\r\n\r\nupload interrupted"
                        }
                    }
                }
            } else {
                let mut body = alloc::vec::Vec::with_capacity(length);
                body.extend_from_slice(&head[start..n]);
                while body.len() < length {
                    match socket.read(&mut chunk).await {
                        Ok(0) | Err(_) => break,
                        Ok(k) => body.extend_from_slice(&chunk[..k.min(length - body.len())]),
                    }
                }
                if body.len() == length {
                    inbox.borrow_mut().upload = Some((title, body));
                    b"HTTP/1.1 202 Accepted\r\nContent-Type: text/plain\r\nConnection: close\r\n\r\nsaving"
                } else {
                    b"HTTP/1.1 400 Bad Request\r\nContent-Type: text/plain\r\nConnection: close\r\n\r\nupload interrupted"
                }
            };
            let _ = socket.write_all(reply).await;
            let _ = socket.flush().await;
            socket.close();
            Timer::after(Duration::from_millis(20)).await;
            socket.abort();
            continue;
        }
        if method == "POST" && (path == "/books/delete" || path == "/books/clear") {
            // Deleting needs the page's explicit confirmation in the request.
            let param = |name: &str| query.split('&').find_map(|kv| kv.strip_prefix(name)?.strip_prefix('='));
            let ok = if path == "/books/delete" {
                match (param("i").and_then(|v| v.parse::<usize>().ok()), param("confirm")) {
                    (Some(i), Some("yes")) => {
                        inbox.borrow_mut().delete = Some(i);
                        true
                    }
                    _ => false,
                }
            } else {
                let place = match param("place") {
                    Some("card") => Some(reader_core::app::Place::Card),
                    Some("memory") => Some(reader_core::app::Place::Flash),
                    _ => None,
                };
                match (place, param("confirm")) {
                    (Some(p), Some("DELETE")) => {
                        inbox.borrow_mut().clear = Some(p);
                        true
                    }
                    _ => false,
                }
            };
            let reply: &[u8] = if ok {
                b"HTTP/1.1 202 Accepted\r\nContent-Type: text/plain\r\nConnection: close\r\n\r\ndeleting"
            } else {
                b"HTTP/1.1 400 Bad Request\r\nContent-Type: text/plain\r\nConnection: close\r\n\r\nnot confirmed"
            };
            let _ = socket.write_all(reply).await;
            let _ = socket.flush().await;
            socket.close();
            Timer::after(Duration::from_millis(20)).await;
            socket.abort();
            continue;
        }

        let r = respond(&mut app.borrow_mut(), method, target, now);

        let mut header = String::new();
        let _ = write!(header, "HTTP/1.1 {}\r\nContent-Type: {}\r\nCache-Control: no-store\r\nConnection: close\r\n", r.status, r.ctype);
        if let Some(h) = &r.extra_header {
            let _ = write!(header, "{h}\r\n");
        }
        header.push_str("\r\n");
        let mut ok = socket.write_all(header.as_bytes()).await.is_ok();
        match r.body {
            Body::Static(s) => ok &= socket.write_all(s.as_bytes()).await.is_ok(),
            Body::Text(s) => ok &= socket.write_all(s.as_bytes()).await.is_ok(),
            Body::Bytes(b) => ok &= socket.write_all(&b).await.is_ok(),
            Body::Raw(b) => ok &= socket.write_all(b).await.is_ok(),
            Body::Book { from, to } => {
                let mut at = from;
                while ok && at < to {
                    let mut piece = String::new();
                    at = book_piece(&app.borrow(), at, to, &mut piece);
                    ok = socket.write_all(piece.as_bytes()).await.is_ok();
                }
            }
        }
        let _ = socket.flush().await;
        socket.close();
        Timer::after(Duration::from_millis(20)).await;
        socket.abort();
    }
}
