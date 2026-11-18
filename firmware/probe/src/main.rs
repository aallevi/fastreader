//! Hardware probe for the ES3C28P: lights the display in four colours with a
//! white border, scans the I2C buses, reads the battery ADC and prints touch
//! points, all over USB serial. `cargo run --release --bin probe`
#![no_std]
#![no_main]

use embedded_graphics::pixelcolor::Rgb565;
use embedded_graphics::prelude::*;
use embedded_graphics::primitives::{PrimitiveStyle, Rectangle};
use embedded_hal_bus::spi::ExclusiveDevice;
use esp_backtrace as _;
use esp_hal::analog::adc::{Adc, AdcConfig, Attenuation};
use esp_hal::clock::CpuClock;
use esp_hal::delay::Delay;
use esp_hal::gpio::{Level, Output, OutputConfig};
use esp_hal::i2c::master::{Config as I2cConfig, I2c};
use esp_hal::main;
use esp_hal::spi::master::{Config as SpiConfig, Spi};
use esp_hal::spi::Mode;
use esp_hal::time::Rate;
use esp_println::println;
use mipidsi::interface::SpiInterface;
use mipidsi::models::ILI9341Rgb565;
use mipidsi::options::{ColorInversion, ColorOrder, Orientation, Rotation};

esp_bootloader_esp_idf::esp_app_desc!();

#[main]
fn main() -> ! {
    let p = esp_hal::init(esp_hal::Config::default().with_cpu_clock(CpuClock::max()));
    let mut delay = Delay::new();
    let out = OutputConfig::default();
    println!("probe: start");

    let mut bl = Output::new(p.GPIO45, Level::Low, out);
    let spi = Spi::new(p.SPI2, SpiConfig::default().with_frequency(Rate::from_mhz(40)).with_mode(Mode::_0))
        .unwrap()
        .with_sck(p.GPIO12)
        .with_mosi(p.GPIO11)
        .with_miso(p.GPIO13);
    let cs = Output::new(p.GPIO10, Level::High, out);
    let dc = Output::new(p.GPIO46, Level::Low, out);
    let spi = ExclusiveDevice::new_no_delay(spi, cs).unwrap();
    let mut buf = [0u8; 1024];
    let mut lcd = mipidsi::Builder::new(ILI9341Rgb565, SpiInterface::new(spi, dc, &mut buf))
        .display_size(240, 320)
        .color_order(ColorOrder::Bgr)
        .invert_colors(ColorInversion::Inverted)
        .orientation(Orientation::new().rotate(Rotation::Deg90))
        .init(&mut delay)
        .unwrap();
    lcd.clear(Rgb565::BLACK).unwrap();
    for (i, c) in [Rgb565::RED, Rgb565::GREEN, Rgb565::BLUE, Rgb565::WHITE].into_iter().enumerate() {
        Rectangle::new(Point::new(i as i32 * 80, 0), Size::new(80, 240)).into_styled(PrimitiveStyle::with_fill(c)).draw(&mut lcd).unwrap();
    }
    Rectangle::new(Point::zero(), Size::new(320, 240)).into_styled(PrimitiveStyle::with_stroke(Rgb565::YELLOW, 3)).draw(&mut lcd).unwrap();
    bl.set_high();
    println!("probe: display drawn (expect red|green|blue|white bars, yellow border)");

    let mut tp_rst = Output::new(p.GPIO18, Level::Low, out);
    delay.delay_millis(10);
    tp_rst.set_high();
    delay.delay_millis(300);
    let mut i2c = I2c::new(p.I2C0, I2cConfig::default().with_frequency(Rate::from_khz(400))).unwrap().with_sda(p.GPIO16).with_scl(p.GPIO15);
    let mut found = [0u8; 8];
    let mut nf = 0;
    for addr in 0x08u8..0x78 {
        if i2c.write(addr, &[]).is_ok() && nf < found.len() {
            found[nf] = addr;
            nf += 1;
        }
    }

    let mut adc_cfg = AdcConfig::new();
    let mut bat = adc_cfg.enable_pin(p.GPIO9, Attenuation::_11dB);
    let mut adc = Adc::new(p.ADC1, adc_cfg);

    let mut n = 0u32;
    loop {
        let mut b = [0u8; 5];
        if i2c.write_read(0x38, &[0x02], &mut b).is_ok() && b[0] & 0x0F > 0 {
            let x = ((b[1] as u16 & 0x0F) << 8) | b[2] as u16;
            let y = ((b[3] as u16 & 0x0F) << 8) | b[4] as u16;
            println!("probe: touch {} pts raw x={} y={}", b[0] & 0x0F, x, y);
        }
        if n % 100 == 0 {
            let raw: u16 = nb::block!(adc.read_oneshot(&mut bat)).unwrap_or(0);
            println!("probe: display ok; i2c devices {:02x?}; battery adc raw={}", &found[..nf], raw);
        }
        n += 1;
        delay.delay_millis(20);
    }
}
