//! The Wi-Fi web view: an access point called "FastReader", a DHCP server so
//! phones get an address, and three HTTP workers that serve the same routes
//! as the simulator (`reader_ui::web`). Started on demand from the menu.
//!
//! With a home network saved (web page > Home Wi-Fi), the reader also joins
//! it as a station, gets an address by DHCP and answers multicast DNS as
//! `fastreader.local`, with two more HTTP workers there. The FastReader
//! network keeps working either way.

use core::cell::RefCell;
use core::fmt::Write as _;
use core::net::{Ipv4Addr, SocketAddrV4};

use alloc::format;
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
use esp_radio::wifi::sta::StationConfig;
use esp_radio::wifi::{AuthenticationMethodConfig, Config, ControllerConfig, Interface, WifiController};
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
    /// Save (Some) or forget (None) the home network, then restart.
    pub home: Option<Option<Home>>,
}

/// The home network to join, saved in flash.
#[derive(Clone, PartialEq, Eq)]
pub struct Home {
    pub ssid: String,
    pub password: String,
}

impl Home {
    /// Flash record: "HW1", ssid length, ssid, password length, password, xor.
    pub fn to_bytes(&self) -> alloc::vec::Vec<u8> {
        let mut v = b"HW1".to_vec();
        v.push(self.ssid.len() as u8);
        v.extend_from_slice(self.ssid.as_bytes());
        v.push(self.password.len() as u8);
        v.extend_from_slice(self.password.as_bytes());
        let x = v.iter().fold(0x5Au8, |a, b| a ^ b);
        v.push(x);
        v
    }

    pub fn from_bytes(b: &[u8]) -> Option<Home> {
        let b = b.strip_prefix(b"HW1")?;
        let sl = *b.first()? as usize;
        let ssid = core::str::from_utf8(b.get(1..1 + sl)?).ok()?;
        let pl = *b.get(1 + sl)? as usize;
        let password = core::str::from_utf8(b.get(2 + sl..2 + sl + pl)?).ok()?;
        let x = b"HW1".iter().chain(&b[..2 + sl + pl]).fold(0x5Au8, |a, c| a ^ c);
        (*b.get(2 + sl + pl)? == x && !ssid.is_empty()).then(|| Home { ssid: ssid.into(), password: password.into() })
    }

    /// A network name of 1 to 32 bytes and a password that is empty (open
    /// network) or 8 to 63 characters.
    pub fn valid(&self) -> bool {
        (1..=32).contains(&self.ssid.len()) && (self.password.is_empty() || (8..=63).contains(&self.password.len()))
    }
}

use crate::mk_static;

pub const SSID: &str = "FastReader";
pub const IP: Ipv4Addr = Ipv4Addr::new(192, 168, 4, 1);
/// Shown in Device info.
pub const ADDRESS: &str = "Wi-Fi FastReader, http://192.168.4.1";

const HTTP_WORKERS: usize = 3;
/// Workers on the home network (fewer: memory is shared with the access point).
const HOME_WORKERS: usize = 2;

/// Brings up the access point (and the home network, if one is saved) and
/// spawns the network tasks.
/// Returns the home network's name when it is being joined: the caller
/// records it (it holds the app while handling the effect that got here).
pub fn start(spawner: Spawner, wifi: WIFI<'static>, app: &'static RefCell<App>, inbox: &'static RefCell<Inbox>, sd: card::Shared, clock: Instant, home: Option<Home>) -> Option<String> {
    println!("wifi: starting (internal heap free {} KB)", esp_alloc::HEAP.free_caps(esp_alloc::MemoryCapability::Internal.into()) / 1024);
    let ap_config = AccessPointConfig::default().with_ssid(SSID.try_into().unwrap());
    let station = home.as_ref().and_then(|h| {
        // WPA2 Personal: what home routers offer (alone, or next to WPA3).
        let auth = if h.password.is_empty() { AuthenticationMethodConfig::Open } else { AuthenticationMethodConfig::Wpa2Personal(h.password.as_str().try_into().ok()?) };
        Some(StationConfig::default().with_ssid(h.ssid.as_str().try_into().ok()?).with_authentication(auth))
    });
    let ap = match &station {
        Some(sta) => Config::AccessPointStation(sta.clone(), ap_config.clone()),
        None => Config::AccessPoint(ap_config.clone()),
    };
    let device = Interface::access_point();
    let controller = match WifiController::new(wifi, ControllerConfig::default().with_initial_config(ap)) {
        Ok(c) => c,
        Err(e) => {
            println!("wifi: failed to start: {:?}", e);
            return None;
        }
    };
    let config = embassy_net::Config::ipv4_static(StaticConfigV4 { address: Ipv4Cidr::new(IP, 24), gateway: Some(IP), dns_servers: Default::default() });
    let rng = Rng::new();
    let seed = (rng.random() as u64) << 32 | rng.random() as u64;
    // Sockets: one per HTTP worker, plus DHCP.
    let (stack, runner) = embassy_net::new(device, config, mk_static!(StackResources<6>, StackResources::<6>::new()), seed);
    if let (Some(h), Some(_)) = (&home, &station) {
        // The home network: DHCP for an address, the name "fastreader".
        let mut dhcp = embassy_net::DhcpConfig::default();
        dhcp.hostname = "fastreader".try_into().ok();
        let seed2 = seed.rotate_left(17) ^ 0x9E37_79B9_7F4A_7C15;
        let (home_stack, home_runner) = embassy_net::new(Interface::station(), embassy_net::Config::dhcpv4(dhcp), mk_static!(StackResources<6>, StackResources::<6>::new()), seed2);
        spawner.spawn(station_link(controller, app, station.clone().unwrap(), ap_config).unwrap());
        spawner.spawn(home_net_task(home_runner).unwrap());
        spawner.spawn(home_address(home_stack, app).unwrap());
        spawner.spawn(mdns(home_stack).unwrap());
        for _ in 0..HOME_WORKERS {
            spawner.spawn(http(home_stack, app, inbox, sd, clock).unwrap());
        }
    } else {
        spawner.spawn(connection(controller).unwrap());
    }
    spawner.spawn(net_task(runner).unwrap());
    spawner.spawn(dhcp(stack).unwrap());
    spawner.spawn(dns(stack).unwrap());
    for _ in 0..HTTP_WORKERS {
        spawner.spawn(http(stack, app, inbox, sd, clock).unwrap());
    }
    println!("wifi: access point {} up, http://{}", SSID, IP);
    home.map(|h| h.ssid)
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

#[embassy_executor::task]
async fn home_net_task(mut runner: Runner<'static, Interface>) {
    runner.run().await
}

/// Joins the home network and rejoins whenever it drops (router restart,
/// out of range). Every try takes the radio off the FastReader channel, so
/// after a few refusals in a row it stops trying (until the next restart or
/// a new password) rather than keep slowing the FastReader network down.
#[embassy_executor::task]
async fn station_link(mut controller: WifiController<'static>, app: &'static RefCell<App>, sta: StationConfig, ap: AccessPointConfig) {
    const TRIES: u32 = 3;
    // Log how the network presents itself: its security mode tells a
    // WPA3-only router (not supported yet) from a wrong password.
    let ssid = app.borrow().device.home_ssid.clone();
    let scan = controller.scan_async(&esp_radio::wifi::scan::ScanConfig::default()).await;
    match &scan {
        Ok(found) => println!("wifi: scan found {} networks", found.len()),
        Err(e) => println!("wifi: scan failed: {:?}", e),
    }
    if let Ok(found) = scan {
        if let Some(home) = found.iter().filter(|a| format!("{:?}", a.ssid).trim_matches('"') == ssid.as_str()).max_by_key(|a| a.signal_strength) {
            println!("wifi: home network seen: {:?}", home);
            // The chip has one radio: the FastReader network must sit on the
            // router's channel, or the router's answers are missed.
            let both = Config::AccessPointStation(sta.clone().with_channel(home.channel), ap.clone().with_channel(home.channel));
            match controller.set_config(&both) {
                Ok(()) => println!("wifi: FastReader moved to channel {}", home.channel),
                Err(e) => println!("wifi: could not move to channel {}: {:?}", home.channel, e),
            }
        }
    }
    let mut failures = 0;
    loop {
        match controller.connect_async().await {
            Ok(info) => {
                println!("wifi: joined home network {:?}", info);
                failures = 0;
                app.borrow_mut().device.home = "getting an address…".into();
                let _ = controller.wait_for_disconnect_async().await;
                println!("wifi: home network dropped");
                app.borrow_mut().device.home = "lost, rejoining…".into();
                Timer::after(Duration::from_secs(5)).await;
            }
            Err(e) => {
                failures += 1;
                println!("wifi: could not join home network ({failures}/{TRIES}): {:?}", e);
                let why = alloc::format!("{:?}", e);
                let why = if why.contains("NoAccessPointFound") {
                    "network not found"
                } else if why.contains("Auth") || why.contains("Handshake") || why.contains("4Way") || why.contains("Mic") {
                    "the router refused it (wrong password, or a WPA3-only router?)"
                } else {
                    "can't join"
                };
                if failures >= TRIES {
                    app.borrow_mut().device.home = alloc::format!("{why}. Stopped trying; save the password again or restart to retry.");
                    let _ = controller.disconnect_async().await;
                    loop {
                        // Keep the controller (and the FastReader network) alive.
                        Timer::after(Duration::from_secs(3600)).await;
                    }
                }
                app.borrow_mut().device.home = alloc::format!("{why}, trying again…");
                Timer::after(Duration::from_secs(10)).await;
            }
        }
    }
}

/// Shows the home address in Device info once DHCP has given one.
#[embassy_executor::task]
async fn home_address(stack: Stack<'static>, app: &'static RefCell<App>) {
    loop {
        stack.wait_config_up().await;
        if let Some(c) = stack.config_v4() {
            let ip = c.address.address();
            println!("wifi: home address {}, http://{}", ip, reader_ui::web::MDNS_NAME);
            app.borrow_mut().device.home = alloc::format!("http://{} ({})", reader_ui::web::MDNS_NAME, ip);
        }
        stack.wait_config_down().await;
    }
}

/// Multicast DNS on the home network: answers "fastreader.local" with the
/// reader's address, so it can be typed in any browser there.
#[embassy_executor::task]
async fn mdns(stack: Stack<'static>) {
    use embassy_net::udp::{PacketMetadata, UdpSocket};
    const GROUP: Ipv4Addr = Ipv4Addr::new(224, 0, 0, 251);
    let mut rx_meta = [PacketMetadata::EMPTY; 4];
    let mut tx_meta = [PacketMetadata::EMPTY; 4];
    let mut rx = [0u8; 1024];
    let mut tx = [0u8; 512];
    let mut socket = UdpSocket::new(stack, &mut rx_meta, &mut rx, &mut tx_meta, &mut tx);
    if socket.bind(5353).is_err() {
        println!("mdns: could not bind");
        return;
    }
    let mut query = [0u8; 1024];
    let mut reply = [0u8; 512];
    loop {
        stack.wait_config_up().await;
        if let Err(e) = stack.join_multicast_group(GROUP) {
            println!("mdns: could not join the group: {:?}", e);
        }
        let Some(ip) = stack.config_v4().map(|c| c.address.address().octets()) else { continue };
        loop {
            let Ok((n, meta)) = socket.recv_from(&mut query).await else { continue };
            if let Some(len) = reader_ui::web::mdns_reply(&query[..n], reader_ui::web::MDNS_NAME, ip, &mut reply) {
                // Normal mDNS asks from port 5353 and is answered to the group;
                // one-shot ("legacy") queries come from other ports and get a direct reply.
                let to = if meta.endpoint.port == 5353 { (GROUP, 5353).into() } else { meta.endpoint };
                let _ = socket.send_to(&reply[..len], to).await;
            }
            if !stack.is_config_up() {
                break; // address lost: wait for the new one
            }
        }
    }
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
#[embassy_executor::task(pool_size = 5)]
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
        if method == "POST" && (path == "/wifi" || path == "/wifi/forget") {
            // The home network: name and password in a small form body.
            let length = head_text
                .lines()
                .find_map(|l| l.to_ascii_lowercase().strip_prefix("content-length:").and_then(|v| v.trim().parse::<usize>().ok()))
                .unwrap_or(0)
                .min(400);
            let start = head_text.find("\r\n\r\n").map_or(n, |p| p + 4);
            let mut body = alloc::vec::Vec::from(&head[start.min(n)..n]);
            while body.len() < length {
                let mut b = [0u8; 256];
                match socket.read(&mut b).await {
                    Ok(0) | Err(_) => break,
                    Ok(k) => body.extend_from_slice(&b[..k]),
                }
            }
            let form = core::str::from_utf8(&body).unwrap_or("");
            let field = |name: &str| form.split('&').find_map(|kv| kv.strip_prefix(name)?.strip_prefix('=')).map(url_decode).unwrap_or_default();
            let reply: &[u8] = if path == "/wifi/forget" {
                inbox.borrow_mut().home = Some(None);
                b"HTTP/1.1 202 Accepted\r\nContent-Type: text/plain\r\nConnection: close\r\n\r\nforgetting; the reader restarts"
            } else {
                let home = Home { ssid: field("ssid"), password: field("password") };
                if home.valid() {
                    inbox.borrow_mut().home = Some(Some(home));
                    b"HTTP/1.1 202 Accepted\r\nContent-Type: text/plain\r\nConnection: close\r\n\r\nsaved; the reader restarts"
                } else {
                    b"HTTP/1.1 400 Bad Request\r\nContent-Type: text/plain\r\nConnection: close\r\n\r\na network name, and a password of 8 to 63 characters (or none)"
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

        app.borrow_mut().web_seen(now);
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
