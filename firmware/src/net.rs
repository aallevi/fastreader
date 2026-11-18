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
use reader_ui::web::{book_piece, respond, Body};

use crate::mk_static;

pub const SSID: &str = "FastReader";
pub const IP: Ipv4Addr = Ipv4Addr::new(192, 168, 4, 1);
/// Shown in Device info.
pub const ADDRESS: &str = "Wi-Fi FastReader → http://192.168.4.1";

const HTTP_WORKERS: usize = 3;

/// Brings up the access point and spawns the network tasks.
pub fn start(spawner: Spawner, wifi: WIFI<'static>, app: &'static RefCell<App>, clock: Instant) {
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
    for _ in 0..HTTP_WORKERS {
        spawner.spawn(http(stack, app, clock).unwrap());
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
    loop {
        let _ = io::server::run(&mut Server::<_, 16>::new_with_et(IP), &ServerOptions::new(IP, Some(&mut gw)), &mut socket, &mut buf).await;
        Timer::after(Duration::from_millis(500)).await;
    }
}

/// One HTTP/1.0-style worker: read the request line, answer through the
/// shared handler, close. The app is borrowed only between awaits.
#[embassy_executor::task(pool_size = 3)]
async fn http(stack: Stack<'static>, app: &'static RefCell<App>, clock: Instant) {
    let mut rx = [0u8; 1024];
    let mut tx = [0u8; 2048];
    loop {
        let mut socket = TcpSocket::new(stack, &mut rx, &mut tx);
        socket.set_timeout(Some(Duration::from_secs(10)));
        if socket.accept(IpListenEndpoint { addr: None, port: 80 }).await.is_err() {
            continue;
        }
        let mut head = [0u8; 1024];
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
        let line = core::str::from_utf8(&head[..n]).unwrap_or("").lines().next().unwrap_or("");
        let mut parts = line.split_whitespace();
        let (method, target) = (parts.next().unwrap_or(""), parts.next().unwrap_or("/"));
        let now = clock.elapsed().as_millis();
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
