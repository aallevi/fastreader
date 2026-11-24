//! Fast reader firmware for the LCDWIKI ES3C28P.
//!
//! All reading logic and drawing live in `reader-core` / `reader-ui` and are
//! tested on the desktop; this file wires up the hardware: display flush,
//! touch, BOOT button, backlight, battery, saving settings, the Wi-Fi web
//! view, and the loop. Everything runs as async tasks on one executor: the
//! screen loop, plus (when the web view is on) the network stack, a DHCP
//! server and three HTTP workers. They share the app through a RefCell that
//! is only ever borrowed between awaits.
#![no_std]
#![no_main]

extern crate alloc;

mod board;
mod card;
mod library;
mod net;

use core::cell::RefCell;

use alloc::format;
use embedded_graphics::pixelcolor::raw::RawU16;
use embedded_graphics::pixelcolor::Rgb565;
use embedded_hal_bus::spi::ExclusiveDevice;
use esp_backtrace as _;
use esp_hal::analog::adc::{Adc, AdcConfig, Attenuation};
use esp_hal::clock::CpuClock;
use esp_hal::delay::Delay;
use esp_hal::gpio::{DriveMode, Input, InputConfig, Level, Output, OutputConfig, Pull};
use esp_hal::i2c::master::{Config as I2cConfig, I2c};
use esp_hal::ledc::channel::{self, ChannelIFace};
use esp_hal::ledc::timer::{self, TimerIFace};
use esp_hal::ledc::{LSGlobalClkSource, Ledc, LowSpeed};
use embassy_executor::Spawner;
use embassy_time::{Duration, Timer};
use esp_hal::timer::timg::TimerGroup;
use esp_hal::spi::master::{Config as SpiConfig, Spi};
use esp_hal::spi::Mode;
use esp_hal::time::{Instant, Rate};
use esp_println::println;
use esp_storage::FlashStorage;
use mipidsi::interface::SpiInterface;
use mipidsi::models::ILI9341Rgb565;
use mipidsi::options::{ColorInversion, ColorOrder, Orientation, Rotation};
use reader_core::app::{Battery, DeviceInfo};
use reader_core::gestures::{Button, Event, Touch};
use reader_core::settings::SETTINGS_LEN;
use reader_core::{App, Book, Effect, Settings};
use reader_ui::{Canvas, Ui, CATALOG};

use board::{HEIGHT, SPI_MHZ, WIDTH};

esp_bootloader_esp_idf::esp_app_desc!();

// Books come from the SD card and uploads; only a welcome text is built in (build.rs).
include!(concat!(env!("OUT_DIR"), "/books.rs"));

/// Settings and per-book positions live in the NVS partition (otherwise unused).
const SETTINGS_ADDR: u32 = 0x9000;
const POSITIONS_ADDR: u32 = 0xA000;
const BOOKMARKS_ADDR: u32 = 0xB000;
/// The home Wi-Fi network (name and password), if one is saved.
const HOME_ADDR: u32 = 0xC000;
const MAX_BOOKMARKS: usize = 800;
const MAX_BOOKS: usize = 64;

const TOUCH_ADDR: u8 = 0x38;

/// Makes a `&'static mut` from a value, once (for state shared with tasks).
#[macro_export]
macro_rules! mk_static {
    ($t:ty, $val:expr) => {{
        static STATIC_CELL: static_cell::StaticCell<$t> = static_cell::StaticCell::new();
        STATIC_CELL.uninit().write($val)
    }};
}

#[esp_hal::main]
async fn main(spawner: Spawner) -> ! {
    // 160 MHz: plenty for a word at a time, and a third less than 240.
    let p = esp_hal::init(esp_hal::Config::default().with_cpu_clock(CpuClock::_160MHz));
    // PSRAM is registered first, so ordinary allocations (books, canvas,
    // glyphs) land there and the internal RAM stays free for the Wi-Fi
    // driver, which asks for internal memory explicitly.
    // The ES3C28P has 8 MB of octal PSRAM; say so instead of auto-detecting,
    // and run it at 80 MHz (the default 40 MHz halves every pixel operation).
    esp_alloc::psram_allocator!(
        p.PSRAM,
        esp_hal::psram,
        esp_hal::psram::PsramConfig {
            mode: esp_hal::psram::PsramMode::OctalSpi,
            size: esp_hal::psram::PsramSize::Size(8 * 1024 * 1024),
            ram_frequency: esp_hal::psram::SpiRamFreq::Freq80m,
            ..Default::default()
        }
    );
    // Internal RAM: the Wi-Fi driver needs it (DMA); reclaimed = bootloader's RAM.
    esp_alloc::heap_allocator!(#[esp_hal::ram(reclaimed)] size: 64 * 1024);
    esp_alloc::heap_allocator!(size: 96 * 1024);
    let timg0 = TimerGroup::new(p.TIMG0);
    esp_rtos::start(timg0.timer0, p.FROM_CPU_INTR0);
    let mut wifi_peripheral = Some(p.WIFI);
    let mut delay = Delay::new();
    let out = OutputConfig::default();
    println!("fastreader {}{}: boot, heap {} KB free", env!("CARGO_PKG_VERSION"), env!("FR_BUILD"), esp_alloc::HEAP.free() / 1024);

    // --- Backlight (PWM, so brightness works).
    let mut ledc = Ledc::new(p.LEDC);
    ledc.set_global_slow_clock(LSGlobalClkSource::APBClk);
    let mut bl_timer = ledc.timer::<LowSpeed>(timer::Number::Timer0);
    bl_timer
        .configure(timer::config::Config { duty: timer::config::Duty::Duty8Bit, clock_source: timer::LSClockSource::APBClk, frequency: Rate::from_khz(12) })
        .unwrap();
    let mut backlight = ledc.channel(channel::Number::Channel0, p.GPIO45);
    backlight.configure(channel::config::Config { timer: &bl_timer, duty_pct: 0, drive_mode: DriveMode::PushPull }).unwrap();

    // --- Display: ILI9341V over SPI. The IPS panel needs inversion (probe-confirmed).
    let spi = Spi::new(p.SPI2, SpiConfig::default().with_frequency(Rate::from_mhz(SPI_MHZ)).with_mode(Mode::_0))
        .unwrap()
        .with_sck(p.GPIO12)
        .with_mosi(p.GPIO11)
        .with_miso(p.GPIO13);
    let cs = Output::new(p.GPIO10, Level::High, out);
    let dc = Output::new(p.GPIO46, Level::Low, out);
    let spi = ExclusiveDevice::new_no_delay(spi, cs).unwrap();
    let mut spi_buf = [0u8; 4096];
    let mut lcd = mipidsi::Builder::new(ILI9341Rgb565, SpiInterface::new(spi, dc, &mut spi_buf))
        .display_size(240, 320)
        .color_order(ColorOrder::Bgr)
        .invert_colors(ColorInversion::Inverted)
        .orientation(orientation(false))
        .init(&mut delay)
        .unwrap();

    // --- microSD card on its own SPI bus (SPI3), so the display isn't disturbed.
    let sd_spi = Spi::new(p.SPI3, SpiConfig::default().with_frequency(Rate::from_khz(400)).with_mode(Mode::_0))
        .unwrap()
        .with_sck(p.GPIO38)
        .with_mosi(p.GPIO40)
        .with_miso(p.GPIO39);
    let sd_cs = Output::new(p.GPIO47, Level::High, out);
    let card_cell: card::Shared = mk_static!(RefCell<Option<card::Card>>, RefCell::new(card::init(sd_spi, sd_cs)));
    let mut card_books = card_cell.borrow_mut().as_mut().map(|c| c.scan()).unwrap_or_default();
    match &*card_cell.borrow() {
        Some(c) if !c.present() => println!("card: none in the slot"),
        Some(c) => println!("card: {} MB, {}, {} books", c.total_mb, if c.formatted() { "FAT" } else { "not formatted" }, card_books.len()),
        None => println!("card: none"),
    }

    // --- Touch (FT6336G).
    let mut tp_rst = Output::new(p.GPIO18, Level::Low, out);
    delay.delay_millis(10);
    tp_rst.set_high();
    delay.delay_millis(300);
    let mut i2c = I2c::new(p.I2C0, I2cConfig::default().with_frequency(Rate::from_khz(400))).unwrap().with_sda(p.GPIO16).with_scl(p.GPIO15);
    let touch_ok = i2c.write(TOUCH_ADDR, &[]).is_ok();
    let mut boot = Input::new(p.GPIO0, InputConfig::default().with_pull(Pull::Up));
    // The touch chip pulls its interrupt line low on a touch: a second way to wake.
    let mut touch_int = Input::new(p.GPIO17, InputConfig::default().with_pull(Pull::Up));
    let mut low_power = esp_hal::rtc_cntl::sleep::LowPower::new(p.LPWR);
    // Optional physical buttons, each between the pin and GND (internal
    // pull-ups; an unwired pin simply reads "not pressed"). See board.rs.
    let up = Input::new(p.GPIO2, InputConfig::default().with_pull(Pull::Up));
    let down = Input::new(p.GPIO3, InputConfig::default().with_pull(Pull::Up));
    let left = Input::new(p.GPIO14, InputConfig::default().with_pull(Pull::Up));
    let right = Input::new(p.GPIO21, InputConfig::default().with_pull(Pull::Up));
    let mut keys = [
        reader_core::gestures::HeldKey::new(reader_core::Key::Up),
        reader_core::gestures::HeldKey::new(reader_core::Key::Down),
        reader_core::gestures::HeldKey::new(reader_core::Key::Left),
        reader_core::gestures::HeldKey::new(reader_core::Key::Right),
    ];

    // --- Battery.
    let mut adc_cfg = AdcConfig::new();
    let mut bat_pin = adc_cfg.enable_pin(p.GPIO9, Attenuation::_11dB);
    let mut adc = Adc::new(p.ADC1, adc_cfg);

    // --- Settings.
    let mut flash = FlashStorage::new(p.FLASH);
    let mut buf = [0u8; SETTINGS_LEN];
    let settings = flash.read(SETTINGS_ADDR, &mut buf).ok().and_then(|_| Settings::from_bytes(&buf)).unwrap_or_default();
    let mut saved = settings.to_bytes();

    // --- App.
    let mut settings = settings;
    // Library = books built into the firmware, then books uploaded to the
    // reader's flash, then books on the SD card.
    let mut uploads = library::scan(&mut flash);
    settings.book = settings.book.min((BOOKS.len() + uploads.len() + card_books.len()).saturating_sub(1) as u8);
    let open_book = |i: u8, flash: &mut FlashStorage, uploads: &[library::Stored], card_books: &[card::Stored]| -> Book {
        let i = i as usize;
        if i < BOOKS.len() {
            let (title, bytes) = BOOKS[i];
            return Book::from_bytes(bytes, title);
        }
        if let Some(s) = uploads.get(i - BOOKS.len()) {
            return match library::read(flash, s) {
                Some(bytes) => Book::from_bytes(&bytes, &s.title),
                None => Book::new("Missing book", "# Missing book\nThis uploaded book could not be read."),
            };
        }
        let Some(s) = card_books.get(i - BOOKS.len() - uploads.len()) else {
            if uploads.is_empty() && card_books.is_empty() {
                // No books anywhere: the welcome text says how to add some.
                return Book::from_bytes(WELCOME, "Welcome");
            }
            // A stand-in: reading it never moves the book's saved place.
            let mut book = Book::new("SD card out", "# The SD card is out\nThis book is on the SD card. Put the card back to go on reading where you were, or choose another book in the Library.");
            book.stand_in = true;
            return book;
        };
        // Card book: only the text comes into memory; pictures stay on the card.
        let opened = card_cell.borrow_mut().as_mut().and_then(|c| c.open_text(s));
        match opened {
            Some((text, pictures)) => {
                let mut book = Book::parse(&alloc::string::String::from_utf8_lossy(&text), &s.title);
                if let Some((base, table)) = pictures {
                    book.images = reader_core::book::Pictures::Stored(card::CardPictures::new(card_cell, s.file.clone(), base, table));
                }
                book
            }
            None => Book::new("Missing book", "# Missing book\nThis book could not be read from the SD card."),
        }
    };
    let mut app = App::new(open_book(settings.book, &mut flash, &uploads, &card_books), CATALOG, settings);
    app.library = BOOKS
        .iter()
        .map(|(t, bytes)| {
            // Only the head is needed for the title.
            let head = alloc::string::String::from_utf8_lossy(&bytes[..bytes.len().min(200)]);
            Book::parse(&head, t).title
        })
        .chain(uploads.iter().map(|s| s.title.clone()))
        .chain(card_books.iter().map(|s| s.title.clone()))
        .collect();
    app.library_info = uploads
        .iter()
        .map(|s| reader_core::app::BookInfo { place: reader_core::app::Place::Flash, bytes: s.len })
        .chain(card_books.iter().map(|s| reader_core::app::BookInfo { place: reader_core::app::Place::Card, bytes: s.len }))
        .collect();
    app.card = card_cell.borrow().as_ref().and_then(|c| c.status(&card_books));
    println!("library: {} books ({} uploaded to flash, {} on the card)", app.library.len(), uploads.len(), card_books.len());
    app.positions = load_positions(&mut flash);
    app.positions.resize(app.library.len(), 0);
    app.bookmarks = load_bookmarks(&mut flash);
    app.device = DeviceInfo {
        board: board::NAME.into(),
        firmware: format!("fastreader {}{}", env!("CARGO_PKG_VERSION"), env!("FR_BUILD")),
        touch: if touch_ok { "FT6336G ok".into() } else { "not found".into() },
        free_memory_kb: (esp_alloc::HEAP.free() / 1024) as u32,
        web: alloc::string::String::new(),
        ..Default::default()
    };
    // The debug build that keeps Wi-Fi on mustn't switch it off by itself.
    app.web_auto_off = !cfg!(feature = "wifi-boot");
    // Just switched on (or woken): say which book and where.
    app.welcome_back(None, 0);
    let mut cv = Canvas::new(WIDTH, HEIGHT);
    let mut ui = Ui::new(WIDTH, HEIGHT);
    let mut touch = Touch::default();
    let mut button = Button::default();
    let start = Instant::now();
    let mut asleep = false;
    let mut last_battery = 0u64;
    let mut next_card_check = 3_000u64;
    let mut px_buf: alloc::vec::Vec<Rgb565> = alloc::vec::Vec::with_capacity((WIDTH * HEIGHT) as usize);
    apply_display(&mut lcd, &mut delay, &mut backlight, &app.settings);
    println!("fastreader: ready (touch {})", touch_ok);

    // Sends the damaged rectangles to the panel as dithered RGB565.
    macro_rules! flush {
        ($rects:expr) => {
            for r in $rects {
                if r.is_empty() {
                    continue;
                }
                px_buf.clear();
                px_buf.extend(cv.rgb565(r).map(|c| Rgb565::from(RawU16::new(c))));
                let _ = lcd.set_pixels(r.x as u16, r.y as u16, (r.right() - 1) as u16, (r.bottom() - 1) as u16, px_buf.iter().copied());
            }
        };
    }

    #[cfg(feature = "selftest")]
    {
        let before = esp_alloc::HEAP.free();
        for face in 0..28u8 {
            let h0 = esp_alloc::HEAP.free();
            let _ = ui.fonts.measure(face, 20.0, "a");
            println!("selftest: font {} costs {} KB", face, (h0 - esp_alloc::HEAP.free()) / 1024);
        }
        println!("selftest: all fonts {} KB", (before - esp_alloc::HEAP.free()) / 1024);
    }
    #[cfg(feature = "selftest")]
    for i in 0..BOOKS.len() as u8 {
        let t0 = Instant::now();
        app.park_book();
        app.open_book(open_book(i, &mut flash, &uploads, &card_books), i);
        let load = t0.elapsed().as_millis();
        let t1 = Instant::now();
        app.settings.mode = 3;
        app.screen = reader_core::Screen::Page;
        ui.invalidate();
        let _ = ui.paint(&mut app, 0, &mut cv);
        let pages = t1.elapsed().as_millis();
        let t2 = Instant::now();
        app.settings.mode = 0;
        app.screen = reader_core::Screen::Reading;
        ui.invalidate();
        let _ = ui.paint(&mut app, 0, &mut cv);
        let full = t2.elapsed().as_millis();
        let t3 = Instant::now();
        for k in 0..50 {
            app.reader.seek(100 + k, 0);
            let _ = ui.paint(&mut app, 0, &mut cv);
        }
        let word = t3.elapsed().as_micros() / 50;
        println!(
            "selftest: book {} {:?}: {} words, load {} ms, paginate {} ms ({} pages), full paint {} ms, word paint {} us, heap free {} KB",
            i, app.reader.book.title, app.reader.book.len(), load, pages, app.pages.len(), full, word, esp_alloc::HEAP.free() / 1024
        );
    }

    // Each reading mode while playing: time per frame including the SPI push.
    #[cfg(feature = "selftest")]
    for mode in 0..4u16 {
        // Steady reading only: no welcome card, no start-up message.
        app.welcome = None;
        app.notice = None;
        app.park_book();
        app.open_book(open_book(1, &mut flash, &uploads, &card_books), 1);
        let _ = app.set_setting("mode", mode);
        app.screen = if mode == 3 { reader_core::Screen::Page } else { reader_core::Screen::Reading };
        ui.invalidate();
        let mut now = 100_000;
        let damage = ui.paint(&mut app, now, &mut cv);
        flush!(damage);
        app.handle(reader_core::Key::Select, now);
        let (mut total, mut worst, mut frames) = (0u64, 0u64, 0u64);
        let mut each: alloc::vec::Vec<u64> = alloc::vec::Vec::new();
        for step in 0..300 {
            now += 33;
            app.tick(now);
            let t = Instant::now();
            let damage = ui.paint(&mut app, now, &mut cv);
            let pushed = !damage.is_empty();
            flush!(damage);
            // Timed after the first 3 s (the "Reading" message has gone).
            if pushed && step >= 90 {
                let us = t.elapsed().as_micros();
                total += us;
                worst = worst.max(us);
                frames += 1;
                each.push(us);
            }
        }
        app.handle(reader_core::Key::Select, now);
        each.sort_unstable();
        let median = each.get(each.len() / 2).copied().unwrap_or(0);
        println!("selftest: mode {} drew {} frames in 7 s of steady reading, median {} us, avg {} us, worst {} us (incl. SPI)", mode, frames, median, total / frames.max(1), worst);
    }
    #[cfg(feature = "selftest")]
    {
        let _ = app.set_setting("mode", 0);
        app.screen = reader_core::Screen::Reading;
        ui.invalidate();
    }

    // From here the app is shared with the web tasks.
    let app_cell: &'static RefCell<App> = mk_static!(RefCell<App>, RefCell::new(app));
    let inbox: &'static RefCell<net::Inbox> = mk_static!(RefCell<net::Inbox>, RefCell::new(net::Inbox::default()));
    if app_cell.borrow().settings.wifi || cfg!(feature = "wifi-boot") {
        if let Some(w) = wifi_peripheral.take() {
            net::start(spawner, w, app_cell, inbox, card_cell, start, load_home(&mut flash));
            app_cell.borrow_mut().device.web = net::ADDRESS.into();
        }
    }

    loop {
        // Everything up to the sleep at the bottom runs without awaiting, so
        // the web tasks never see the app half-way through a frame.
        let idle = {
        let mut app_guard = app_cell.borrow_mut();
        let app = &mut *app_guard;
        let now = start.elapsed().as_millis();

        // The SD card taken out or put back: a card that's in is checked
        // every 2 s (one block read, even while reading); an empty slot
        // every 4 s, only while paused (starting a card takes longer).
        if now >= next_card_check {
            let present = card_cell.borrow().as_ref().is_some_and(|c| c.present());
            next_card_check = now + if present { 2_000 } else { 4_000 };
            let change = if present || !app.reader.is_playing() { card_cell.borrow_mut().as_mut().and_then(|c| c.check()) } else { None };
            match change {
                Some(card::Change::Removed) => {
                    card_books.clear();
                    app.card = None;
                    let book = app.settings.book;
                    let on_card = app.library_info.get(book as usize).is_some_and(|b| b.place == reader_core::app::Place::Card);
                    if on_card {
                        // Its place is saved; a stand-in page takes its place.
                        app.park_book();
                        save_positions(&mut flash, &app.positions);
                        app.open_book(open_book(book, &mut flash, &uploads, &card_books), book);
                    }
                    app.card_removed(now);
                    ui.invalidate();
                }
                Some(card::Change::Inserted) => {
                    card_books = card_cell.borrow_mut().as_mut().map(|c| c.scan()).unwrap_or_default();
                    app.library = BOOKS
                        .iter()
                        .map(|(t, bytes)| Book::parse(&alloc::string::String::from_utf8_lossy(&bytes[..bytes.len().min(200)]), t).title)
                        .chain(uploads.iter().map(|s| s.title.clone()))
                        .chain(card_books.iter().map(|s| s.title.clone()))
                        .collect();
                    app.library_info = uploads
                        .iter()
                        .map(|s| reader_core::app::BookInfo { place: reader_core::app::Place::Flash, bytes: s.len })
                        .chain(card_books.iter().map(|s| reader_core::app::BookInfo { place: reader_core::app::Place::Card, bytes: s.len }))
                        .collect();
                    app.positions.resize(app.library.len().max(1), 0);
                    app.card = card_cell.borrow().as_ref().and_then(|c| c.status(&card_books));
                    println!("card: {} books", card_books.len());
                    if app.reader.book.stand_in {
                        // Back to the book, where it was.
                        let book = app.settings.book.min(app.library.len().saturating_sub(1) as u8);
                        app.open_book(open_book(book, &mut flash, &uploads, &card_books), book);
                    }
                    app.card_inserted(now);
                    ui.invalidate();
                }
                None => {}
            }
        }

        // Battery every 10 s (and at start).
        if now >= last_battery {
            last_battery = now + 10_000;
            let raw: u16 = nb::block!(adc.read_oneshot(&mut bat_pin)).unwrap_or(0);
            app.battery = board::battery_mv(raw).map(|mv| Battery { percent: board::battery_percent(mv), charging: false, millivolts: mv });
            app.device.free_memory_kb = (esp_alloc::HEAP.free() / 1024) as u32;
        }

        // Touch: first point only, in screen coordinates.
        let finger = if touch_ok {
            let mut b = [0u8; 5];
            match i2c.write_read(TOUCH_ADDR, &[0x02], &mut b) {
                Ok(()) if b[0] & 0x0F > 0 && b[0] & 0x0F < 3 => {
                    let rx = ((b[1] as u16 & 0x0F) << 8) | b[2] as u16;
                    let ry = ((b[3] as u16 & 0x0F) << 8) | b[4] as u16;
                    let (x, y) = board::touch_to_screen(rx, ry, app.settings.flipped);
                    Some(ui.hit(app, x, y))
                }
                _ => None,
            }
        } else {
            None
        };
        let levels = [up.is_low(), down.is_low(), left.is_low(), right.is_low()];
        let events = [
            touch.update(finger, now),
            button.update(boot.is_low(), now),
            keys[0].update(levels[0], now),
            keys[1].update(levels[1], now),
            keys[2].update(levels[2], now),
            keys[3].update(levels[3], now),
        ];
        for ev in events.into_iter().flatten() {
            if asleep {
                // The first touch or press only wakes the screen.
                if matches!(ev, Event::Tap(_) | Event::Press(_)) {
                    asleep = false;
                    let _ = lcd.wake(&mut delay);
                    ui.invalidate();
                    apply_display(&mut lcd, &mut delay, &mut backlight, &app.settings);
                }
                continue;
            }
            match ev {
                Event::Down(hit) => app.touch_down(hit),
                Event::Tap(hit) => {
                    app.tap(hit, now);
                }
                Event::Held(k) => {
                    app.handle_held(k, now);
                }
                Event::Release => app.release(now),
                Event::Cancel => app.touch_cancel(),
                Event::Press(k) => {
                    app.handle(k, now);
                }
            }
        }

        // Books sent from the browser: store them (this loop owns the flash).
        let request = core::mem::take(&mut *inbox.borrow_mut());
        if let Some((title, text)) = request.upload {
            match library::append(&mut flash, &title, &text) {
                Ok(stored) => {
                    println!("library: stored {:?} ({} bytes)", stored.title, stored.len);
                    let bytes = stored.len;
                    uploads.push(stored);
                    app.book_added(title, reader_core::app::BookInfo { place: reader_core::app::Place::Flash, bytes }, now);
                }
                Err(e) => println!("library: {}", e),
            }
        }
        if let Some(home) = request.home {
            // A home network saved or forgotten on the web page: store it,
            // save the place, and restart so the radio comes up with it.
            let record = home.as_ref().map(|h| h.to_bytes()).unwrap_or_else(|| alloc::vec![0xFF; 4]);
            let _ = flash.write(HOME_ADDR, &record);
            println!("wifi: home network {}", if home.is_some() { "saved" } else { "forgotten" });
            let _ = flash.write(SETTINGS_ADDR, &app.settings.to_bytes());
            app.park_book();
            save_positions(&mut flash, &app.positions);
            delay.delay_millis(500); // let the page get its answer
            esp_hal::system::software_reset();
        }
        if let Some(stored) = request.card_added {
            // The web task already wrote it to the card; insert it after the
            // flash uploads, keeping the library in its order.
            println!("card: stored {:?} ({} bytes)", stored.title, stored.len);
            card_books.push(stored.clone());
            app.card = card_cell.borrow().as_ref().and_then(|c| c.status(&card_books));
            let bytes = stored.len;
            app.book_added(stored.title, reader_core::app::BookInfo { place: reader_core::app::Place::Card, bytes }, now);
        }
        // Deleting, confirmed on the page: one book, or every book in one place.
        // Library order: flash uploads first, then the card's books.
        if let Some(i) = request.delete {
            let k = i.wrapping_sub(BOOKS.len());
            let done = if k < uploads.len() {
                let ok = library::delete(&mut flash, &uploads[k]);
                if ok {
                    uploads.remove(k);
                }
                ok
            } else if k - uploads.len() < card_books.len() {
                let c = k - uploads.len();
                let ok = card_cell.borrow_mut().as_mut().is_some_and(|card| card.delete(&card_books[c]));
                if ok {
                    card_books.remove(c);
                }
                ok
            } else {
                false
            };
            if done {
                if app.settings.book as usize == i {
                    app.park_book();
                }
                app.book_removed(i, now);
                app.card = card_cell.borrow().as_ref().and_then(|c| c.status(&card_books));
            }
        }
        if let Some(place) = request.clear {
            let first = BOOKS.len();
            let (from, count) = match place {
                reader_core::app::Place::Flash => {
                    library::clear(&mut flash);
                    (first, core::mem::take(&mut uploads).len())
                }
                _ => {
                    if let Some(c) = card_cell.borrow_mut().as_mut() {
                        c.remove_books();
                    }
                    (first + uploads.len(), core::mem::take(&mut card_books).len())
                }
            };
            for i in (from..from + count).rev() {
                app.book_removed(i, now);
            }
            app.card = card_cell.borrow().as_ref().and_then(|c| c.status(&card_books));
        }

        app.tick(now);
        #[cfg(feature = "sleep-test")]
        if now > 5_000 && now < 5_100 {
            app.screen = reader_core::Screen::Menu { sel: reader_core::app::menu_index(reader_core::app::MenuItem::Sleep) };
            app.handle(reader_core::Key::Select, now);
        }
        while let Some(e) = app.take_effect() {
            match e {
                Effect::SaveSettings => {}
                Effect::Display => {
                    apply_display(&mut lcd, &mut delay, &mut backlight, &app.settings);
                    ui.invalidate();
                }
                Effect::OpenBook(i) => {
                    app.park_book(); // free the old book first: memory is tight
                    app.open_book(open_book(i, &mut flash, &uploads, &card_books), i);
                    save_positions(&mut flash, &app.positions);
                    ui.invalidate();
                }
                Effect::Sleep => {
                    // Real deep sleep (~10 µA for the chip): save everything, screen
                    // off, then wake on BOOT or a touch; the reader restarts where it was.
                    let _ = flash.write(SETTINGS_ADDR, &app.settings.to_bytes());
                    app.positions.resize(app.library.len().max(1), 0);
                    let book = app.settings.book as usize;
                    if !app.reader.book.stand_in {
                        app.positions[book] = app.reader.pos() as u32;
                    }
                    save_positions(&mut flash, &app.positions);
                    let damage = ui.paint(app, now, &mut cv);
                    flush!(damage);
                    delay.delay_millis(800); // let "Going to sleep" be read
                    let _ = backlight.set_duty(0);
                    let _ = lcd.sleep(&mut delay);
                    // Don't fall asleep while the finger or button is still down.
                    while boot.is_low() || touch_int.is_low() {
                        delay.delay_millis(20);
                    }
                    delay.delay_millis(150);
                    let wake = esp_hal::gpio::WakeupConfig::default().with_low_power_path(true);
                    boot.listen(esp_hal::gpio::Event::LowLevel);
                    let _ = boot.apply_wakeup_config(&wake);
                    if touch_ok {
                        touch_int.listen(esp_hal::gpio::Event::LowLevel);
                        let _ = touch_int.apply_wakeup_config(&wake);
                    }
                    println!("fastreader: deep sleep");
                    low_power.sleep_deep(esp_hal::rtc_cntl::sleep::RtcSleepConfig::deep());
                }
                Effect::SaveBookmarks => save_bookmarks(&mut flash, &app.bookmarks),
                // The same path as a delete confirmed on the web page (next turn of the loop).
                Effect::DeleteBook(i) => inbox.borrow_mut().delete = Some(i as usize),
                Effect::Download => {} // the desktop app's update notice only
                Effect::Dim(down) => {
                    // Idle: the backlight to its lowest; back to the setting on a touch.
                    let pct = [8u8, 20, 40, 70, 100][(app.settings.brightness.clamp(1, 5) - 1) as usize];
                    let _ = backlight.set_duty(if down { pct.min(4) } else { pct });
                }
                Effect::FormatCard => {
                    // Show "Formatting the card…" first: clearing the FATs takes a while.
                    let damage = ui.paint(app, now, &mut cv);
                    flush!(damage);
                    let open_on_card = app.settings.book as usize >= BOOKS.len() + uploads.len();
                    if open_on_card {
                        app.park_book();
                    }
                    let id = start.elapsed().as_micros() as u32 ^ 0xFA57_2026;
                    let ok = card_cell.borrow_mut().as_mut().is_some_and(|c| c.format(id, |d, t| println!("card: formatting {}%", d * 100 / t.max(1))));
                    println!("card: format {}", if ok { "done" } else { "failed" });
                    card_books.clear();
                    let info = card_cell.borrow().as_ref().and_then(|c| c.status(&card_books));
                    app.books_removed(BOOKS.len() + uploads.len(), now);
                    app.card_formatted(if ok { info } else { None }, now);
                    if !ok {
                        app.card = info;
                    }
                }
                Effect::Wifi(true) => {
                    if let Some(w) = wifi_peripheral.take() {
                        net::start(spawner, w, app_cell, inbox, card_cell, start, load_home(&mut flash));
                    }
                    app.device.web = net::ADDRESS.into();
                }
                Effect::Wifi(false) => {
                    // The radio can only be fully powered down by a restart:
                    // save everything, show the message, then reset.
                    let _ = flash.write(SETTINGS_ADDR, &app.settings.to_bytes());
                    app.positions.resize(app.library.len().max(1), 0);
                    let book = app.settings.book as usize;
                    if !app.reader.book.stand_in {
                        app.positions[book] = app.reader.pos() as u32;
                    }
                    save_positions(&mut flash, &app.positions);
                    let damage = ui.paint(app, now, &mut cv);
                    flush!(damage);
                    delay.delay_millis(1200);
                    esp_hal::system::software_reset();
                }
            }
        }
        // Write settings only when they changed (flash wear), never while playing fast.
        let now_bytes = app.settings.to_bytes();
        if now_bytes != saved && !app.reader.is_playing() {
            if flash.write(SETTINGS_ADDR, &now_bytes).is_ok() {
                saved = now_bytes;
            }
        }

        if !asleep {
            let damage = ui.paint(app, now, &mut cv);
            let drew = !damage.is_empty();
            flush!(damage);
            if drew {
                // Now really on the panel (a picture may have come from the card).
                app.frame_shown(start.elapsed().as_millis());
            }
        }

        // Poll touch at ~100 Hz; sleep a little longer when idle.
        app.next_wake().map_or(15, |t| t.saturating_sub(start.elapsed().as_millis()).min(10)).max(2)
        };
        Timer::after(Duration::from_millis(idle)).await;
    }
}

/// Landscape, USB on the left. The panel is mounted so that plain 90° shows
/// the image upside down and mirrored (seen on the device), so flip vertically.
fn orientation(flipped: bool) -> Orientation {
    let o = Orientation::new().rotate(if flipped { Rotation::Deg270 } else { Rotation::Deg90 });
    o.flip_vertical()
}

/// Per-book positions: "F2" + count + u32 each.
fn load_positions(flash: &mut FlashStorage) -> alloc::vec::Vec<u32> {
    let mut head = [0u8; 3];
    // "F2": positions of the card-based library (older ones belonged to the
    // built-in books and are left behind).
    if flash.read(POSITIONS_ADDR, &mut head).is_err() || &head[..2] != b"F2" {
        return alloc::vec::Vec::new();
    }
    let n = (head[2] as usize).min(MAX_BOOKS);
    let mut raw = alloc::vec![0u8; n * 4];
    if flash.read(POSITIONS_ADDR + 3, &mut raw).is_err() {
        return alloc::vec::Vec::new();
    }
    raw.chunks(4).map(|c| u32::from_le_bytes([c[0], c[1], c[2], c[3]])).collect()
}

fn save_positions(flash: &mut FlashStorage, pos: &[u32]) {
    let n = pos.len().min(MAX_BOOKS);
    let mut raw = alloc::vec![b'F', b'2', n as u8];
    for p in &pos[..n] {
        raw.extend_from_slice(&p.to_le_bytes());
    }
    let _ = flash.write(POSITIONS_ADDR, &raw);
}

/// Bookmarks: "B2" + count (u16) + (book u8, word u32) each.
/// The saved home Wi-Fi network, if any.
fn load_home(flash: &mut FlashStorage) -> Option<net::Home> {
    let mut buf = [0u8; 104];
    flash.read(HOME_ADDR, &mut buf).ok()?;
    net::Home::from_bytes(&buf)
}

fn load_bookmarks(flash: &mut FlashStorage) -> alloc::vec::Vec<(u8, u32)> {
    let mut head = [0u8; 4];
    if flash.read(BOOKMARKS_ADDR, &mut head).is_err() || &head[..2] != b"B2" {
        return alloc::vec::Vec::new();
    }
    let n = (u16::from_le_bytes([head[2], head[3]]) as usize).min(MAX_BOOKMARKS);
    let mut raw = alloc::vec![0u8; n * 5];
    if flash.read(BOOKMARKS_ADDR + 4, &mut raw).is_err() {
        return alloc::vec::Vec::new();
    }
    raw.chunks(5).map(|c| (c[0], u32::from_le_bytes([c[1], c[2], c[3], c[4]]))).collect()
}

fn save_bookmarks(flash: &mut FlashStorage, marks: &[(u8, u32)]) {
    let n = marks.len().min(MAX_BOOKMARKS);
    let mut raw = alloc::vec![b'B', b'2'];
    raw.extend_from_slice(&(n as u16).to_le_bytes());
    for (b, w) in &marks[..n] {
        raw.push(*b);
        raw.extend_from_slice(&w.to_le_bytes());
    }
    let _ = flash.write(BOOKMARKS_ADDR, &raw);
}

fn apply_display<'a, DI, M, RST, CH>(lcd: &mut mipidsi::Display<DI, M, RST>, delay: &mut Delay, bl: &mut CH, s: &Settings)
where
    DI: mipidsi::interface::Interface,
    M: mipidsi::models::Model,
    M::ColorFormat: mipidsi::interface::InterfacePixelFormat<DI::Word>,
    RST: embedded_hal::digital::OutputPin,
    CH: ChannelIFace<'a, LowSpeed>,
{
    let _ = lcd.set_orientation(orientation(s.flipped));
    let _ = delay;
    let pct = [8u8, 20, 40, 70, 100][(s.brightness.clamp(1, 5) - 1) as usize];
    let _ = bl.set_duty(pct);
}
