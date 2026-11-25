//! LCDWIKI ES3C28P / ES3N28P: ESP32-S3 N16R8, 2.8" 240×320 IPS (ILI9341V,
//! SPI), FT6336G touch, microSD on 4-bit SDMMC, ES8311 audio codec.
//! Pins from the vendor's IO table, confirmed on the board with the probe
//! (I2C devices at 0x18 codec and 0x38 touch; panel needs colour inversion).

pub const NAME: &str = "LCDWIKI ES3C28P (ESP32-S3 N16R8)";
pub const WIDTH: u32 = 320; // landscape, USB on the left
pub const HEIGHT: u32 = 240;
pub const SPI_MHZ: u32 = 60; // the vendor runs it at 60 MHz; drop to 40 on glitches

// LCD: SCK 12, MOSI 11, MISO 13, CS 10, DC 46, RST = EN line, BL 45 (PWM)
// Touch (FT6336G @ 0x38): SDA 16, SCL 15, INT 17, RST 18
// SD (SDMMC 4-bit): CLK 38, CMD 40, D0 39, D1 41, D2 48, D3 47
// Battery divider: GPIO 9 (ADC1)
// Optional buttons (each to GND, internal pull-ups):
//   GPIO 2 = ▲ faster/up, GPIO 3 = ▼ slower/down,
//   GPIO 14 = ◀ back (hold: scroll), GPIO 21 = ▶ forward (hold: scroll),
//   BOOT (GPIO 0) = select (short) / menu (long).

/// Touch reports portrait panel coordinates; turn them into the landscape
/// screen. Measured: top-left tap ≈ raw (190, 15), bottom-right ≈ (40, 300).
pub fn touch_to_screen(raw_x: u16, raw_y: u16, flipped: bool) -> (i32, i32) {
    let x = raw_y as i32;
    let y = 239 - raw_x as i32;
    let (x, y) = (x.clamp(0, WIDTH as i32 - 1), y.clamp(0, HEIGHT as i32 - 1));
    if flipped { (WIDTH as i32 - 1 - x, HEIGHT as i32 - 1 - y) } else { (x, y) }
}

/// The battery pin's calibrated reading (millivolts at GPIO 9, behind the
/// board's 200 kΩ / 200 kΩ divider: half the battery) → battery millivolts.
/// (The uncalibrated reading sat at full scale; the chip's own calibration
/// gives the real value.) Under 1 V at the pin: no battery.
pub fn battery_mv(pin_mv: u16) -> Option<u16> {
    if !(1_000..=2_300).contains(&pin_mv) {
        return None;
    }
    Some(pin_mv * 2)
}

/// LiPo voltage → rough percentage (resting curve).
pub fn battery_percent(mv: u16) -> u8 {
    const CURVE: [(u16, u8); 8] = [(3300, 0), (3500, 5), (3600, 12), (3700, 30), (3800, 55), (3900, 70), (4000, 82), (4150, 100)];
    if mv <= CURVE[0].0 {
        return 0;
    }
    for w in CURVE.windows(2) {
        let ((v0, p0), (v1, p1)) = (w[0], w[1]);
        if mv <= v1 {
            return p0 + ((mv - v0) as u32 * (p1 - p0) as u32 / (v1 - v0) as u32) as u8;
        }
    }
    100
}

// microSD slot, driven in SPI mode on its SDIO pins (LCDWIKI ES3C28P manual:
// SD_CLK 38, SD_CMD 40, SD_D0 39, SD_D3 47; D1 41 and D2 48 unused). Set up
// in main.rs on SPI3; see card.rs.

/// Charging, guessed from the battery voltage (the TP4054's status pin
/// isn't wired to the ESP32): plugging in lifts the voltage at once by the
/// charge current through the cell's resistance, unplugging drops it again.
/// To be checked with the battery: the jump size and how full looks.
#[derive(Default)]
pub struct Charger {
    /// The last few readings (oldest first).
    recent: [u16; 4],
    count: usize,
    charging: bool,
}

impl Charger {
    /// A rise this big over the last few readings: plugged in (the voltage
    /// climbs over a few seconds, not in one step); a fall as big: unplugged.
    const JUMP_MV: u16 = 60;

    pub fn update(&mut self, mv: u16) -> bool {
        if self.count == self.recent.len() {
            self.recent.rotate_left(1);
            self.count -= 1;
        }
        self.recent[self.count] = mv;
        self.count += 1;
        let seen = &self.recent[..self.count];
        let (lo, hi) = (*seen.iter().min().unwrap_or(&mv), *seen.iter().max().unwrap_or(&mv));
        if mv >= lo + Self::JUMP_MV && !self.charging {
            self.charging = true;
            self.reset(mv);
        } else if mv + Self::JUMP_MV <= hi && self.charging {
            self.charging = false;
            self.reset(mv);
        }
        self.charging
    }

    /// After a change, compare only against what comes next.
    fn reset(&mut self, mv: u16) {
        self.recent[0] = mv;
        self.count = 1;
    }
}
