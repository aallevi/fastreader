//! Storage that never needs one big contiguous allocation.
//!
//! On the ESP32 a 170k-word book wants ~1.3 MB of word index and ~1 MB of
//! text; after the heap has been used for a while there may be plenty free
//! but no single block that large. These keep data in 64 KB pieces.

use alloc::string::String;
use alloc::vec::Vec;
use core::ops::{Index, Range};

const BLOCK: usize = 8192; // items per block (64 KB of 8-byte words)

/// A Vec in fixed-size blocks.
pub struct Chunked<T> {
    blocks: Vec<Vec<T>>,
    len: usize,
}

impl<T: Copy> Chunked<T> {
    pub fn new() -> Self {
        Chunked { blocks: Vec::new(), len: 0 }
    }

    pub fn push(&mut self, v: T) {
        if self.len.is_multiple_of(BLOCK) {
            self.blocks.push(Vec::with_capacity(BLOCK));
        }
        self.blocks.last_mut().unwrap().push(v);
        self.len += 1;
    }

    pub fn len(&self) -> usize {
        self.len
    }

    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    pub fn get(&self, i: usize) -> Option<T> {
        (i < self.len).then(|| self.blocks[i / BLOCK][i % BLOCK])
    }

    pub fn set(&mut self, i: usize, v: T) {
        if i < self.len {
            self.blocks[i / BLOCK][i % BLOCK] = v;
        }
    }

    pub fn last_mut(&mut self) -> Option<&mut T> {
        self.blocks.last_mut().and_then(|b| b.last_mut())
    }

    pub fn iter(&self) -> impl Iterator<Item = &T> + '_ {
        self.blocks.iter().flatten()
    }

    pub fn range(&self, r: Range<usize>) -> impl Iterator<Item = &T> + '_ {
        let end = r.end.min(self.len);
        (r.start.min(end)..end).map(move |i| &self.blocks[i / BLOCK][i % BLOCK])
    }

    pub fn shrink_to_fit(&mut self) {
        if let Some(b) = self.blocks.last_mut() {
            b.shrink_to_fit();
        }
    }
}

impl<T: Copy> Default for Chunked<T> {
    fn default() -> Self {
        Self::new()
    }
}

impl<T> Index<usize> for Chunked<T> {
    type Output = T;
    fn index(&self, i: usize) -> &T {
        &self.blocks[i / BLOCK][i % BLOCK]
    }
}

const TEXT_BLOCK: usize = 64 * 1024;

/// Text in ≤64 KB strings; a word never straddles two of them, so every
/// word is still one `&str`. Positions encode (block << 16 | offset).
#[derive(Default)]
pub struct ChunkedText {
    blocks: Vec<String>,
}

impl ChunkedText {
    /// Appends `s` and returns its position.
    pub fn push(&mut self, s: &str) -> u32 {
        let fits = self.blocks.last().is_some_and(|b| b.len() + s.len() <= TEXT_BLOCK);
        if !fits {
            self.blocks.push(String::with_capacity(TEXT_BLOCK.min(s.len().max(4096) * 16)));
        }
        let b = self.blocks.last_mut().unwrap();
        let off = b.len();
        b.push_str(s);
        ((self.blocks.len() - 1) << 16 | off) as u32
    }

    pub fn get(&self, pos: u32, len: usize) -> &str {
        let (b, off) = ((pos >> 16) as usize, (pos & 0xFFFF) as usize);
        &self.blocks[b][off..off + len]
    }

    pub fn shrink_to_fit(&mut self) {
        if let Some(b) = self.blocks.last_mut() {
            b.shrink_to_fit();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chunked_indexes_across_blocks() {
        let mut c = Chunked::new();
        for i in 0..20_000u32 {
            c.push(i);
        }
        assert_eq!(c.len(), 20_000);
        assert_eq!(c[8191], 8191);
        assert_eq!(c[8192], 8192);
        assert_eq!(c.get(19_999), Some(19_999));
        assert_eq!(c.get(20_000), None);
        assert_eq!(c.range(8190..8194).copied().collect::<Vec<_>>(), [8190, 8191, 8192, 8193]);
        assert_eq!(c.iter().count(), 20_000);
    }

    #[test]
    fn text_words_never_straddle_blocks() {
        let mut t = ChunkedText::default();
        let mut pos = Vec::new();
        for i in 0..30_000 {
            let w = alloc::format!("word{i}");
            pos.push((t.push(&w), w));
        }
        assert!(t.blocks.len() > 1);
        for (p, w) in &pos {
            assert_eq!(t.get(*p, w.len()), w);
        }
    }
}
