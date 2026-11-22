//! The microSD card (slot on the back of the board). Talked to in SPI mode on
//! the slot's SDIO pins: CLK GPIO38, CMD GPIO40 (data in), D0 GPIO39 (data
//! out), D3 GPIO47 (chip select); see board.rs.
//!
//! Books live in `BOOKS/` as `BK000001.BK`, … (the driver knows only 8.3
//! names; the real title is inside the file). A book file is the same as an
//! upload: text, then the version-2 picture section, whose table lets one
//! picture be read without the others. Only the text is held in memory; the
//! pictures stay on the card until shown.
//!
//! The card is shared by the screen loop (opening books, pictures) and the
//! web tasks (uploads) through a RefCell, borrowed only between awaits.

use core::cell::RefCell;

use alloc::boxed::Box;
use alloc::string::String;
use alloc::vec::Vec;
use embedded_hal_bus::spi::ExclusiveDevice;
use embedded_sdmmc::{Mode, RawDirectory, RawFile, RawVolume, SdCard, TimeSource, Timestamp, VolumeIdx, VolumeManager};
use esp_hal::delay::Delay;
use esp_hal::gpio::Output;
use esp_hal::spi::master::{Config as SpiConfig, Spi};
use esp_hal::time::Rate;
use esp_hal::Blocking;
use reader_core::book::{image_table, Image, PictureSource, IMAGES_MAGIC, IMAGES_MAGIC2};

pub type Dev = SdCard<ExclusiveDevice<Spi<'static, Blocking>, Output<'static>, Delay>, Delay>;
type Vm = VolumeManager<Dev, Clock, 4, 4, 1>;

/// The card has no clock to stamp files with; a fixed date is fine.
pub struct Clock;
impl TimeSource for Clock {
    fn get_timestamp(&self) -> Timestamp {
        Timestamp { year_since_1970: 56, zero_indexed_month: 0, zero_indexed_day: 0, hours: 0, minutes: 0, seconds: 0 }
    }
}

/// A book on the card.
#[derive(Clone)]
pub struct Stored {
    pub title: String,
    pub file: String,
    pub len: u32,
}

enum State {
    Mounted { vm: Vm, vol: RawVolume, dir: RawDirectory },
    /// A card that isn't FAT (new, exFAT, broken): it can still be formatted.
    Unformatted(Dev),
    Gone,
}

pub struct Card {
    state: State,
    pub total_mb: u32,
}

pub type Shared = &'static RefCell<Option<Card>>;

/// Starts the card at 400 kHz (the SD rule for start-up), then runs it at
/// 20 MHz. `None` without a card.
pub fn init(spi: Spi<'static, Blocking>, cs: Output<'static>) -> Option<Card> {
    let dev = SdCard::new(ExclusiveDevice::new(spi, cs, Delay::new()).ok()?, Delay::new());
    let bytes = dev.num_bytes().ok()?;
    dev.spi(|s| {
        let _ = s.bus_mut().apply_config(&SpiConfig::default().with_frequency(Rate::from_mhz(20)));
    });
    let mut card = Card { state: State::Unformatted(dev), total_mb: (bytes >> 20) as u32 };
    card.mount();
    Some(card)
}

impl Card {
    fn mount(&mut self) {
        let dev = match core::mem::replace(&mut self.state, State::Gone) {
            State::Unformatted(d) => d,
            State::Mounted { vm, .. } => vm.free().0,
            State::Gone => return,
        };
        let vm: Vm = VolumeManager::new_with_limits(dev, Clock, 5000);
        // What is on the card (for the log): partition type and the boot
        // sector's file system name.
        let mut b = [embedded_sdmmc::Block::new()];
        if vm.device(|d| embedded_sdmmc::BlockDevice::read(d, &mut b, embedded_sdmmc::BlockIdx(0))).is_ok() {
            let m = &b[0].contents;
            let start = u32::from_le_bytes([m[454], m[455], m[456], m[457]]);
            esp_println::println!("card: MBR signature {:02x}{:02x}, partition type {:#04x} at sector {}", m[510], m[511], m[450], start);
            let mut bs = [embedded_sdmmc::Block::new()];
            if vm.device(|d| embedded_sdmmc::BlockDevice::read(d, &mut bs, embedded_sdmmc::BlockIdx(start))).is_ok() {
                let c = &bs[0].contents;
                esp_println::println!("card: boot sector OEM {:?}, FAT32 name {:?}, FAT16 name {:?}", core::str::from_utf8(&c[3..11]).unwrap_or("?"), core::str::from_utf8(&c[82..90]).unwrap_or("?"), core::str::from_utf8(&c[54..62]).unwrap_or("?"));
            }
        }
        let opened = (|| {
            let vol = match vm.open_raw_volume(VolumeIdx(0)) {
                Ok(v) => v,
                Err(e) => {
                    esp_println::println!("card: can't open the volume: {:?}", e);
                    return None;
                }
            };
            let root = vm.open_root_dir(vol).ok()?;
            let dir = match vm.open_dir(root, "BOOKS") {
                Ok(d) => d,
                Err(_) => {
                    vm.make_dir_in_dir(root, "BOOKS").ok()?;
                    vm.open_dir(root, "BOOKS").ok()?
                }
            };
            let _ = vm.close_dir(root);
            Some((vol, dir))
        })();
        self.state = match opened {
            Some((vol, dir)) => State::Mounted { vm, vol, dir },
            None => State::Unformatted(vm.free().0),
        };
    }

    pub fn formatted(&self) -> bool {
        matches!(self.state, State::Mounted { .. })
    }

    /// Size and free space for the menu (free = size minus the books).
    pub fn info(&self, books: &[Stored]) -> reader_core::app::Card {
        let used: u64 = books.iter().map(|b| b.len as u64).sum();
        let free = if self.formatted() { self.total_mb.saturating_sub((used >> 20) as u32 + 1) } else { 0 };
        reader_core::app::Card { total_mb: self.total_mb, free_mb: free, formatted: self.formatted() }
    }

    /// The books on the card, in name (= upload) order.
    pub fn scan(&mut self) -> Vec<Stored> {
        let State::Mounted { vm, dir, .. } = &self.state else { return Vec::new() };
        let mut found: Vec<(String, u32)> = Vec::new();
        let _ = vm.iterate_dir(*dir, |e| {
            if e.name.extension() == b"BK" && !e.attributes.is_directory() {
                found.push((alloc::format!("{}", e.name), e.size));
            }
            core::ops::ControlFlow::Continue(())
        });
        found.sort();
        found
            .into_iter()
            .map(|(file, len)| {
                let mut head = [0u8; 300];
                let n = self.read_at(&file, 0, &mut head).unwrap_or(0);
                let head = String::from_utf8_lossy(&head[..n]);
                let title = reader_core::Book::parse(&head, &file).title;
                Stored { title, file, len }
            })
            .collect()
    }

    /// Reads up to `buf.len()` bytes of `file` from `offset`.
    fn read_at(&mut self, file: &str, offset: u32, buf: &mut [u8]) -> Option<usize> {
        let State::Mounted { vm, dir, .. } = &self.state else { return None };
        let f = vm.open_file_in_dir(*dir, file, Mode::ReadOnly).ok()?;
        let r = (|| {
            vm.file_seek_from_start(f, offset).ok()?;
            let mut got = 0;
            while got < buf.len() {
                match vm.read(f, &mut buf[got..]) {
                    Ok(0) | Err(_) => break,
                    Ok(k) => got += k,
                }
            }
            Some(got)
        })();
        let _ = vm.close_file(f);
        r
    }

    /// A book's text (everything before the pictures) and, if it has
    /// version-2 pictures, where they start and their table.
    pub fn open_text(&mut self, book: &Stored) -> Option<(Vec<u8>, Option<(u32, Vec<(u16, u16, u32)>)>)> {
        let mut text = Vec::new();
        let mut chunk = alloc::vec![0u8; 8192];
        let mut at = 0u32;
        while at < book.len {
            let n = self.read_at(&book.file, at, &mut chunk)?;
            if n == 0 {
                break;
            }
            text.extend_from_slice(&chunk[..n]);
            at += n as u32;
            // Stop at the picture section: the pixels stay on the card.
            let from = text.len().saturating_sub(n + IMAGES_MAGIC2.len());
            if let Some(p) = text[from..].windows(IMAGES_MAGIC2.len()).position(|w| w == IMAGES_MAGIC2) {
                let magic = from + p;
                let base = (magic + IMAGES_MAGIC2.len()) as u32;
                text.truncate(magic);
                let mut head = [0u8; 4];
                self.read_at(&book.file, base, &mut head)?;
                let count = u16::from_le_bytes([head[0], head[1]]) as usize;
                let mut table = alloc::vec![0u8; 4 + 8 * count];
                self.read_at(&book.file, base, &mut table)?;
                return Some((text, Some((base, image_table(&table)))));
            }
            // An old-format (version 1) section can't be read piecewise:
            // keep it in memory only if the book is small.
            if text.windows(IMAGES_MAGIC.len()).any(|w| w == IMAGES_MAGIC) && book.len > 3 * 1024 * 1024 {
                let p = text.windows(IMAGES_MAGIC.len()).position(|w| w == IMAGES_MAGIC).unwrap();
                text.truncate(p);
                return Some((text, None));
            }
        }
        Some((text, None))
    }

    /// One picture's pixels.
    pub fn picture(&mut self, file: &str, offset: u32, w: u16, h: u16) -> Option<Image> {
        let mut raw = alloc::vec![0u8; w as usize * h as usize * 2];
        if self.read_at(file, offset, &mut raw)? < raw.len() {
            return None;
        }
        Some(Image { w, h, px: raw.chunks_exact(2).map(|c| u16::from_le_bytes([c[0], c[1]])).collect() })
    }

    /// Opens a new book file for an upload; returns it and its name.
    pub fn create(&mut self) -> Option<(RawFile, String)> {
        let next = self.scan().iter().filter_map(|b| b.file.get(2..8)?.parse::<u32>().ok()).max().unwrap_or(0) + 1;
        let State::Mounted { vm, dir, .. } = &self.state else { return None };
        let name = alloc::format!("BK{next:06}.BK");
        let f = vm.open_file_in_dir(*dir, name.as_str(), Mode::ReadWriteCreateOrTruncate).ok()?;
        Some((f, name))
    }

    pub fn write(&mut self, f: RawFile, data: &[u8]) -> bool {
        let State::Mounted { vm, .. } = &self.state else { return false };
        vm.write(f, data).is_ok()
    }

    /// Closes an upload; `keep: false` deletes it (interrupted).
    pub fn finish(&mut self, f: RawFile, name: &str, keep: bool) {
        let State::Mounted { vm, dir, .. } = &self.state else { return };
        let _ = vm.close_file(f);
        if !keep {
            let _ = vm.delete_entry_in_dir(*dir, name);
        }
    }

    /// Deletes every book on the card.
    pub fn remove_books(&mut self) {
        let books = self.scan();
        let State::Mounted { vm, dir, .. } = &self.state else { return };
        for b in books {
            let _ = vm.delete_entry_in_dir(*dir, b.file.as_str());
        }
    }

    /// Erases the card and formats it FAT32, then mounts it again.
    pub fn format(&mut self, volume_id: u32, progress: impl FnMut(u32, u32)) -> bool {
        let dev = match core::mem::replace(&mut self.state, State::Gone) {
            State::Mounted { vm, vol, dir } => {
                let _ = vm.close_dir(dir);
                let _ = vm.close_volume(vol);
                vm.free().0
            }
            State::Unformatted(d) => d,
            State::Gone => return false,
        };
        let ok = fat_format::format(&dev, volume_id, progress).is_ok();
        self.state = State::Unformatted(dev);
        self.mount();
        ok && self.formatted()
    }
}

/// A card book's pictures, fetched when shown. Keeps the last one, since a
/// picture stays on screen for several redraws.
pub struct CardPictures {
    pub card: Shared,
    pub file: String,
    pub base: u32,
    pub table: Vec<(u16, u16, u32)>,
    last: RefCell<Option<(usize, Image)>>,
}

impl CardPictures {
    pub fn new(card: Shared, file: String, base: u32, table: Vec<(u16, u16, u32)>) -> Box<Self> {
        Box::new(CardPictures { card, file, base, table, last: RefCell::new(None) })
    }
}

// One core, one executor: the source never actually moves between threads.
unsafe impl Send for CardPictures {}

impl PictureSource for CardPictures {
    fn len(&self) -> usize {
        self.table.len()
    }
    fn dims(&self, n: usize) -> Option<(u16, u16)> {
        self.table.get(n).map(|&(w, h, _)| (w, h))
    }
    fn load(&self, n: usize) -> Option<Image> {
        if let Some((k, img)) = &*self.last.borrow() {
            if *k == n {
                return Some(img.clone());
            }
        }
        let &(w, h, off) = self.table.get(n)?;
        let img = self.card.try_borrow_mut().ok()?.as_mut()?.picture(&self.file, self.base + off, w, h)?;
        *self.last.borrow_mut() = Some((n, img.clone()));
        Some(img)
    }
}
