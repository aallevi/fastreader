//! Fast reader firmware for the LCDWIKI ES3C28P.
//!
//! All reading logic and drawing live in `reader-core` / `reader-ui` and are
//! tested on the desktop; this file wires up the hardware: display flush,
//! touch, BOOT button, backlight, battery, saving settings, and the loop.
#![no_std]
#![no_main]

extern crate alloc;

mod board;

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
use esp_hal::main;
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

// Books are built into flash (see build.rs) until the SD card library lands.
include!(concat!(env!("OUT_DIR"), "/books.rs"));

/// Settings and per-book positions live in the NVS partition (otherwise unused).
const SETTINGS_ADDR: u32 = 0x9000;
const POSITIONS_ADDR: u32 = 0xA000;
const MAX_BOOKS: usize = 64;

const TOUCH_ADDR: u8 = 0x38;

#[main]
fn main() -> ! {
    let p = esp_hal::init(esp_hal::Config::default().with_cpu_clock(CpuClock::max()));
    esp_alloc::heap_allocator!(size: 120 * 1024);
    // Book text, the 300 KB canvas and the glyph cache live in PSRAM.
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
    let mut delay = Delay::new();
    let out = OutputConfig::default();
    println!("fastreader: boot, heap {} KB free", esp_alloc::HEAP.free() / 1024);

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

    // --- Touch (FT6336G).
    let mut tp_rst = Output::new(p.GPIO18, Level::Low, out);
    delay.delay_millis(10);
    tp_rst.set_high();
    delay.delay_millis(300);
    let mut i2c = I2c::new(p.I2C0, I2cConfig::default().with_frequency(Rate::from_khz(400))).unwrap().with_sda(p.GPIO16).with_scl(p.GPIO15);
    let touch_ok = i2c.write(TOUCH_ADDR, &[]).is_ok();
    let boot = Input::new(p.GPIO0, InputConfig::default().with_pull(Pull::Up));

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
    settings.book = settings.book.min(BOOKS.len().saturating_sub(1) as u8);
    let open = |i: u8| {
        let (title, text) = BOOKS[i as usize];
        Book::parse(text, title)
    };
    let mut app = App::new(open(settings.book), CATALOG, settings);
    app.library = BOOKS.iter().map(|(t, text)| Book::parse(&text[..text.len().min(200)], t).title).collect();
    app.positions = load_positions(&mut flash);
    app.positions.resize(BOOKS.len(), 0);
    app.device = DeviceInfo {
        board: board::NAME.into(),
        firmware: format!("fastreader {}", env!("CARGO_PKG_VERSION")),
        touch: if touch_ok { "FT6336G ok".into() } else { "not found".into() },
        free_memory_kb: (esp_alloc::HEAP.free() / 1024) as u32,
    };
    let mut cv = Canvas::new(WIDTH, HEIGHT);
    let mut ui = Ui::new(WIDTH, HEIGHT);
    let mut touch = Touch::default();
    let mut button = Button::default();
    let start = Instant::now();
    let mut asleep = false;
    let mut last_battery = 0u64;
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
        app.open_book(open(i), i);
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
        app.park_book();
        app.open_book(open(1), 1);
        let _ = app.set_setting("mode", mode);
        app.screen = if mode == 3 { reader_core::Screen::Page } else { reader_core::Screen::Reading };
        ui.invalidate();
        let mut now = 100_000;
        let damage = ui.paint(&mut app, now, &mut cv);
        flush!(damage);
        app.handle(reader_core::Key::Select, now);
        let (mut total, mut worst, mut frames) = (0u64, 0u64, 0u64);
        for _ in 0..90 {
            now += 33;
            app.tick(now);
            let t = Instant::now();
            let damage = ui.paint(&mut app, now, &mut cv);
            let pushed = !damage.is_empty();
            flush!(damage);
            if pushed {
                let us = t.elapsed().as_micros();
                total += us;
                worst = worst.max(us);
                frames += 1;
            }
        }
        app.handle(reader_core::Key::Select, now);
        println!("selftest: mode {} drew {} of 90 frames, avg {} us, worst {} us (incl. SPI)", mode, frames, total / frames.max(1), worst);
    }
    #[cfg(feature = "selftest")]
    {
        let _ = app.set_setting("mode", 0);
        app.screen = reader_core::Screen::Reading;
        ui.invalidate();
    }

    loop {
        let now = start.elapsed().as_millis();

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
                    Some(ui.hit(&app, x, y))
                }
                _ => None,
            }
        } else {
            None
        };
        let events = [touch.update(finger, now), button.update(boot.is_low(), now)];
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

        app.tick(now);
        while let Some(e) = app.take_effect() {
            match e {
                Effect::SaveSettings => {}
                Effect::Display => {
                    apply_display(&mut lcd, &mut delay, &mut backlight, &app.settings);
                    ui.invalidate();
                }
                Effect::OpenBook(i) => {
                    app.park_book(); // free the old book first: memory is tight
                    app.open_book(open(i), i);
                    save_positions(&mut flash, &app.positions);
                    ui.invalidate();
                }
                Effect::Sleep => {
                    let _ = backlight.set_duty(0);
                    let _ = lcd.sleep(&mut delay);
                    asleep = true;
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
            let damage = ui.paint(&mut app, now, &mut cv);
            flush!(damage);
        }

        // Poll touch at ~100 Hz; sleep a little longer when idle.
        let idle = app.next_wake().map_or(15, |t| t.saturating_sub(start.elapsed().as_millis()).min(10));
        delay.delay_millis(idle.max(2) as u32);
    }
}

/// Landscape, USB on the left. The panel is mounted so that plain 90° shows
/// the image upside down and mirrored (seen on the device), so flip vertically.
fn orientation(flipped: bool) -> Orientation {
    let o = Orientation::new().rotate(if flipped { Rotation::Deg270 } else { Rotation::Deg90 });
    o.flip_vertical()
}

/// Per-book positions: "FP" + count + u32 each.
fn load_positions(flash: &mut FlashStorage) -> alloc::vec::Vec<u32> {
    let mut head = [0u8; 3];
    if flash.read(POSITIONS_ADDR, &mut head).is_err() || &head[..2] != b"FP" {
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
    let mut raw = alloc::vec![b'F', b'P', n as u8];
    for p in &pos[..n] {
        raw.extend_from_slice(&p.to_le_bytes());
    }
    let _ = flash.write(POSITIONS_ADDR, &raw);
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
