//! Formats a disk image file, to check the result with the computer's own
//! tools: `cargo run -p fat-format --example format_image -- card.img 300`
//! (size in MB), then `hdiutil attach` / `fsck_msdos` on macOS.

use embedded_sdmmc::{Block, BlockCount, BlockDevice, BlockIdx};
use std::cell::RefCell;
use std::fs::File;
use std::io::{Read, Seek, SeekFrom, Write};

struct Image(RefCell<File>, u32);

impl BlockDevice for Image {
    type Error = std::io::Error;
    fn read(&self, blocks: &mut [Block], start: BlockIdx) -> Result<(), Self::Error> {
        let mut f = self.0.borrow_mut();
        f.seek(SeekFrom::Start(start.0 as u64 * 512))?;
        for b in blocks {
            f.read_exact(&mut b.contents)?;
        }
        Ok(())
    }
    fn write(&self, blocks: &[Block], start: BlockIdx) -> Result<(), Self::Error> {
        let mut f = self.0.borrow_mut();
        f.seek(SeekFrom::Start(start.0 as u64 * 512))?;
        for b in blocks {
            f.write_all(&b.contents)?;
        }
        Ok(())
    }
    fn num_blocks(&self) -> Result<BlockCount, Self::Error> {
        Ok(BlockCount(self.1))
    }
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let mb: u64 = args.get(2).and_then(|s| s.parse().ok()).unwrap_or(300);
    let f = File::options().read(true).write(true).create(true).truncate(true).open(&args[1]).expect("image");
    f.set_len(mb << 20).unwrap();
    let dev = Image(RefCell::new(f), (mb << 20) as u32 / 512);
    let r = fat_format::format(&dev, 0xFA57_2026, |d, t| eprint!("\rclearing FATs {}%", d * 100 / t)).expect("format");
    eprintln!("\n{r:?}");
}
