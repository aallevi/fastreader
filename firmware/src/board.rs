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

/// Raw ADC (11 dB attenuation, 12-bit) → battery millivolts, assuming the
/// usual 1:2 divider. A pinned-high reading means no battery (USB only).
pub fn battery_mv(raw: u16) -> Option<u16> {
    if raw >= 4080 || raw < 1000 {
        return None;
    }
    Some((raw as u32 * 3100 * 2 / 4095) as u16)
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
    last: Option<u16>,
    charging: bool,
}

impl Charger {
    /// A sudden rise from one reading to the next.
    const JUMP_MV: u16 = 40;

    pub fn update(&mut self, mv: u16) -> bool {
        if let Some(last) = self.last {
            if mv >= last + Self::JUMP_MV {
                self.charging = true;
            } else if mv + Self::JUMP_MV <= last {
                self.charging = false;
            }
        }
        self.last = Some(mv);
        self.charging
    }
}
