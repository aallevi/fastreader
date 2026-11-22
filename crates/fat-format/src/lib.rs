//! Formats a block device (an SD card) as one FAT32 partition behind an MBR,
//! the layout every computer and phone reads. `no_std`, no allocation.
//!
//! Layout, following Microsoft's FAT32 specification ("FAT: General Overview
//! of On-Disk Format", 2000):
//! - sector 0: MBR with one partition (type 0x0C, FAT32 LBA) starting at 1 MiB;
//! - partition: boot sector, FSInfo, backup boot sector at 6, backup FSInfo at
//!   7, 32 reserved sectors in all; two FATs; data, root directory in cluster 2.

#![no_std]

use embedded_sdmmc::{Block, BlockCount, BlockDevice, BlockIdx};

/// Partition start: 1 MiB, the usual alignment for flash erase blocks.
const START: u32 = 2048;
const RESERVED: u32 = 32;
const FATS: u32 = 2;

/// What the format produced (for "32 GB, 31.9 GB free").
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Formatted {
    pub sectors_per_cluster: u32,
    pub clusters: u32,
    pub fat_sectors: u32,
    pub partition_sectors: u32,
}

#[derive(Debug, PartialEq, Eq)]
pub enum Error<E> {
    /// Too small for FAT32 (needs at least 65 525 clusters, about 33 MB).
    TooSmall,
    Device(E),
}

/// Cluster size by volume size, as Windows formats FAT32 (in 512-byte sectors).
fn sectors_per_cluster(sectors: u32) -> u32 {
    match sectors {
        0..=532_480 => 1,             // up to 260 MB: 512 B
        532_481..=16_777_216 => 8,    // up to 8 GB: 4 KB
        16_777_217..=33_554_432 => 16, // up to 16 GB: 8 KB
        33_554_433..=67_108_864 => 32, // up to 32 GB: 16 KB
        _ => 64,                       // larger: 32 KB
    }
}

/// Sectors per FAT, the specification's formula.
fn fat_size(sectors: u32, spc: u32) -> u32 {
    let tmp1 = (sectors - RESERVED) as u64;
    let tmp2 = (256 * spc as u64 + FATS as u64) / 2;
    tmp1.div_ceil(tmp2) as u32
}

fn put16(b: &mut [u8], at: usize, v: u16) {
    b[at..at + 2].copy_from_slice(&v.to_le_bytes());
}
fn put32(b: &mut [u8], at: usize, v: u32) {
    b[at..at + 4].copy_from_slice(&v.to_le_bytes());
}

/// Formats `dev`, erasing everything on it. `volume_id` should differ per
/// format (a clock value is fine). `progress(done, total)` reports the slow
/// part (clearing the FATs).
pub fn format<D: BlockDevice>(dev: &D, volume_id: u32, mut progress: impl FnMut(u32, u32)) -> Result<Formatted, Error<D::Error>> {
    let BlockCount(total) = dev.num_blocks().map_err(Error::Device)?;
    if total <= START + RESERVED + 1024 {
        return Err(Error::TooSmall);
    }
    let size = total - START;
    let spc = sectors_per_cluster(size);
    let fat = fat_size(size, spc);
    let clusters = (size - RESERVED - FATS * fat) / spc;
    if clusters < 65_525 {
        return Err(Error::TooSmall);
    }
    let write = |idx: u32, b: &Block| dev.write(core::slice::from_ref(b), BlockIdx(idx)).map_err(Error::Device);

    // MBR: one FAT32 (LBA) partition.
    let mut mbr = Block::new();
    {
        let e = &mut mbr.contents[446..462];
        e[0] = 0x00; // not bootable
        e[1..4].copy_from_slice(&[0xFE, 0xFF, 0xFF]); // CHS: use LBA
        e[4] = 0x0C;
        e[5..8].copy_from_slice(&[0xFE, 0xFF, 0xFF]);
        put32(e, 8, START);
        put32(e, 12, size);
    }
    mbr.contents[510] = 0x55;
    mbr.contents[511] = 0xAA;

    // Boot sector (BPB).
    let mut boot = Block::new();
    {
        let b = &mut boot.contents;
        b[0..3].copy_from_slice(&[0xEB, 0x58, 0x90]);
        b[3..11].copy_from_slice(b"MSWIN4.1");
        put16(b, 11, 512);
        b[13] = spc as u8;
        put16(b, 14, RESERVED as u16);
        b[16] = FATS as u8;
        put16(b, 17, 0); // root entries: 0 on FAT32
        put16(b, 19, 0); // total sectors (16-bit): 0
        b[21] = 0xF8; // fixed disk
        put16(b, 22, 0); // FAT size (16-bit): 0
        put16(b, 24, 63);
        put16(b, 26, 255);
        put32(b, 28, START); // hidden sectors before the partition
        put32(b, 32, size);
        put32(b, 36, fat);
        put16(b, 40, 0); // FATs mirrored
        put16(b, 42, 0); // version 0.0
        put32(b, 44, 2); // root directory cluster
        put16(b, 48, 1); // FSInfo sector
        put16(b, 50, 6); // backup boot sector
        b[64] = 0x80;
        b[66] = 0x29;
        put32(b, 67, volume_id);
        b[71..82].copy_from_slice(b"FASTREADER ");
        b[82..90].copy_from_slice(b"FAT32   ");
        b[510] = 0x55;
        b[511] = 0xAA;
    }

    // FSInfo: free clusters (all but the root's) and where to look next.
    let mut info = Block::new();
    {
        let b = &mut info.contents;
        put32(b, 0, 0x4161_5252);
        put32(b, 484, 0x6141_7272);
        put32(b, 488, clusters - 1);
        put32(b, 492, 3);
        put32(b, 508, 0xAA55_0000);
    }

    // Clear the FATs and the root directory cluster first, then write the
    // structures: an interrupted format leaves no half-valid volume.
    let zero = [Block::new(), Block::new(), Block::new(), Block::new(), Block::new(), Block::new(), Block::new(), Block::new()];
    let fat_start = START + RESERVED;
    let data_start = fat_start + FATS * fat;
    let clear = FATS * fat + spc;
    let mut done = 0;
    while done < clear {
        let n = (clear - done).min(zero.len() as u32);
        dev.write(&zero[..n as usize], BlockIdx(fat_start + done)).map_err(Error::Device)?;
        done += n;
        if done % 2048 < n || done == clear {
            progress(done, clear);
        }
    }
    debug_assert_eq!(data_start, fat_start + FATS * fat);

    // FAT entries 0 and 1 (media, clean), 2 (root directory, end of chain).
    let mut first = Block::new();
    put32(&mut first.contents, 0, 0x0FFF_FFF8);
    put32(&mut first.contents, 4, 0x0FFF_FFFF);
    put32(&mut first.contents, 8, 0x0FFF_FFFF);
    for k in 0..FATS {
        write(fat_start + k * fat, &first)?;
    }
    write(START + 1, &info)?;
    write(START + 7, &info)?;
    write(START + 6, &boot)?;
    write(START, &boot)?;
    // Erase the old partition table last of all: the MBR makes it real.
    write(0, &mbr)?;
    Ok(Formatted { sectors_per_cluster: spc, clusters, fat_sectors: fat, partition_sectors: size })
}

#[cfg(test)]
mod tests {
    extern crate std;
    use super::*;
    use core::cell::RefCell;
    use embedded_sdmmc::{Mode, TimeSource, Timestamp, VolumeIdx, VolumeManager};
    use std::vec;
    use std::vec::Vec;

    /// A card in memory.
    struct Ram(RefCell<Vec<u8>>);

    impl BlockDevice for Ram {
        type Error = core::convert::Infallible;
        fn read(&self, blocks: &mut [Block], start: BlockIdx) -> Result<(), Self::Error> {
            let d = self.0.borrow();
            for (k, b) in blocks.iter_mut().enumerate() {
                let at = (start.0 as usize + k) * 512;
                b.contents.copy_from_slice(&d[at..at + 512]);
            }
            Ok(())
        }
        fn write(&self, blocks: &[Block], start: BlockIdx) -> Result<(), Self::Error> {
            let mut d = self.0.borrow_mut();
            for (k, b) in blocks.iter().enumerate() {
                let at = (start.0 as usize + k) * 512;
                d[at..at + 512].copy_from_slice(&b.contents);
            }
            Ok(())
        }
        fn num_blocks(&self) -> Result<BlockCount, Self::Error> {
            Ok(BlockCount((self.0.borrow().len() / 512) as u32))
        }
    }

    struct Clock;
    impl TimeSource for Clock {
        fn get_timestamp(&self) -> Timestamp {
            Timestamp { year_since_1970: 56, zero_indexed_month: 10, zero_indexed_day: 21, hours: 9, minutes: 0, seconds: 0 }
        }
    }

    fn card(mb: usize) -> Ram {
        // Old junk everywhere, as on a used card.
        Ram(RefCell::new(vec![0xA5; mb << 20]))
    }

    #[test]
    fn a_formatted_card_holds_files() {
        let dev = card(64);
        let mut steps = 0;
        let f = format(&dev, 0x1234_5678, |_, _| steps += 1).unwrap();
        assert!(f.clusters >= 65_525 && steps > 0);
        let vm: VolumeManager<_, _> = VolumeManager::new(dev, Clock);
        let vol = vm.open_volume(VolumeIdx(0)).unwrap();
        let root = vol.open_root_dir().unwrap();
        let mut n = 0;
        root.iterate_dir(|_| { n += 1; core::ops::ControlFlow::Continue(()) }).unwrap();
        assert_eq!(n, 0, "a fresh card is empty");
        root.make_dir_in_dir("BOOKS").unwrap();
        let books = root.open_dir("BOOKS").unwrap();
        let data: Vec<u8> = (0..300_000u32).map(|i| (i * 7) as u8).collect();
        {
            let file = books.open_file_in_dir("GOMBRICH.BK", Mode::ReadWriteCreate).unwrap();
            file.write(&data).unwrap();
            file.close().unwrap();
        }
        let file = books.open_file_in_dir("GOMBRICH.BK", Mode::ReadOnly).unwrap();
        let mut back = vec![0u8; data.len()];
        let mut got = 0;
        while got < back.len() {
            got += file.read(&mut back[got..]).unwrap();
        }
        assert_eq!(back, data, "what was written reads back");
    }

    #[test]
    fn layout_matches_the_specification() {
        let dev = card(64);
        let f = format(&dev, 7, |_, _| {}).unwrap();
        let d = dev.0.borrow();
        assert_eq!(&d[510..512], &[0x55, 0xAA]);
        assert_eq!(d[446 + 4], 0x0C, "FAT32 LBA partition");
        let boot = &d[START as usize * 512..][..512];
        assert_eq!(&boot[82..90], b"FAT32   ");
        assert_eq!(u16::from_le_bytes([boot[11], boot[12]]), 512);
        assert_eq!(boot[13] as u32, f.sectors_per_cluster);
        let backup = &d[(START as usize + 6) * 512..][..512];
        assert_eq!(boot, backup, "backup boot sector");
        assert_eq!(sectors_per_cluster(62_333_952), 32, "a 32 GB card gets 16 KB clusters");
    }

    #[test]
    fn tiny_devices_are_refused() {
        assert_eq!(format(&card(8), 1, |_, _| {}), Err(Error::TooSmall));
    }
}
