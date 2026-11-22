//! Books uploaded from the browser, stored in the `books` flash partition
//! (see partitions.csv). A simple log: each book starts on a 4 KB sector
//!
//!   "BK" · version 1 · 0 · title length (u16) · text length (u32) · title · text
//!
//! and the log ends at the first sector without the magic. Uploads append;
//! "remove uploaded books" erases the first sector, which empties the log.
//! Deleting one book clears its version byte to 0 (flash can clear bits
//! without erasing): the log still steps over it, the library skips it.

use alloc::string::String;
use alloc::vec;
use alloc::vec::Vec;
use esp_storage::FlashStorage;

pub const ADDR: u32 = 0x71_0000;
pub const SIZE: u32 = 0x8F_0000;
const SECTOR: u32 = 4096;
const HEADER: u32 = 10;

#[derive(Clone)]
pub struct Stored {
    /// Flash offset of the entry's header.
    pub at: u32,
    pub title: String,
    /// Flash offset of the text.
    pub text_at: u32,
    pub len: u32,
}

fn align_up(v: u32, a: u32) -> u32 {
    v.div_ceil(a) * a
}

/// Every stored book, in upload order.
pub fn scan(flash: &mut FlashStorage) -> Vec<Stored> {
    let mut out = Vec::new();
    let mut at = ADDR;
    while at + HEADER < ADDR + SIZE {
        let mut h = [0u8; HEADER as usize];
        if flash.read(at, &mut h).is_err() || &h[..2] != b"BK" || h[2] > 1 {
            break;
        }
        let deleted = h[2] == 0;
        let tlen = u16::from_le_bytes([h[4], h[5]]) as u32;
        let len = u32::from_le_bytes([h[6], h[7], h[8], h[9]]);
        if at + HEADER + tlen + len > ADDR + SIZE {
            break;
        }
        let mut title = vec![0u8; tlen as usize];
        let _ = flash.read(at + HEADER, &mut title);
        if !deleted {
            out.push(Stored { at, title: String::from_utf8_lossy(&title).into_owned(), text_at: at + HEADER + tlen, len });
        }
        at = align_up(at + HEADER + tlen + len, SECTOR);
    }
    out
}

/// Where the next book would start, and how many bytes are free.
pub fn free(flash: &mut FlashStorage) -> (u32, u32) {
    // The end of the log, deleted entries included (they still take room).
    let mut at = ADDR;
    loop {
        let mut h = [0u8; HEADER as usize];
        if at + HEADER >= ADDR + SIZE || flash.read(at, &mut h).is_err() || &h[..2] != b"BK" || h[2] > 1 {
            break;
        }
        let tlen = u16::from_le_bytes([h[4], h[5]]) as u32;
        let len = u32::from_le_bytes([h[6], h[7], h[8], h[9]]);
        at = align_up(at + HEADER + tlen + len, SECTOR);
    }
    (at, ADDR + SIZE - at)
}

/// Deletes one book: its version byte becomes 0 (bits only cleared).
pub fn delete(flash: &mut FlashStorage, s: &Stored) -> bool {
    flash.write(s.at + 2, &[0]).is_ok()
}

/// Appends a book. Erases exactly the sectors it needs, writes aligned.
pub fn append(flash: &mut FlashStorage, title: &str, text: &[u8]) -> Result<Stored, &'static str> {
    let (at, room) = free(flash);
    let title = &title.as_bytes()[..title.len().min(200)];
    let total = HEADER + title.len() as u32 + text.len() as u32;
    if total > room {
        return Err("not enough room for this book");
    }
    flash.erase(at, align_up(at + total, SECTOR)).map_err(|_| "flash erase failed")?;
    // Header + title + text as one stream, written in word-aligned pieces.
    let mut head = Vec::with_capacity((HEADER as usize) + title.len());
    head.extend_from_slice(b"BK\x01\x00");
    head.extend_from_slice(&(title.len() as u16).to_le_bytes());
    head.extend_from_slice(&(text.len() as u32).to_le_bytes());
    head.extend_from_slice(title);
    let mut stream = head.iter().chain(text.iter()).copied();
    let mut buf = [0u8; 4096];
    let mut off = at;
    loop {
        let mut n = 0;
        for b in stream.by_ref().take(buf.len()) {
            buf[n] = b;
            n += 1;
        }
        if n == 0 {
            break;
        }
        let padded = n.div_ceil(4) * 4;
        buf[n..padded].fill(0xFF);
        flash.write_nor(off, &buf[..padded]).map_err(|_| "flash write failed")?;
        off += padded as u32;
    }
    Ok(Stored { at, title: String::from_utf8_lossy(title).into_owned(), text_at: at + HEADER + title.len() as u32, len: text.len() as u32 })
}

/// A stored book's file bytes (text, then optional images), into PSRAM.
pub fn read(flash: &mut FlashStorage, s: &Stored) -> Option<Vec<u8>> {
    let mut v = vec![0u8; s.len as usize];
    flash.read(s.text_at, &mut v).ok()?;
    Some(v)
}

/// Removes every uploaded book.
pub fn clear(flash: &mut FlashStorage) {
    let _ = flash.erase(ADDR, ADDR + SECTOR);
}
