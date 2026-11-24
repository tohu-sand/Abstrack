//! Recently drawn pages, kept compressed so that scrolling back costs neither
//! a new rendering nor memory in the web view.
//!
//! Pages of a paper are mostly white; deflate shrinks a page bitmap to a few
//! hundredths of its size. A bitmap is cut into parts that are compressed on
//! separate threads, each from a fresh state and ended on a byte boundary:
//! laid end to end they are one deflate stream, which a web view inflates as
//! it is, and each part can also be inflated on its own, side by side.

use flate2::{Compress, Compression, Decompress, FlushCompress, FlushDecompress, Status};
use std::collections::VecDeque;
use std::sync::Arc;

/// Total compressed size kept.
const BUDGET: usize = 24 * 1024 * 1024;
/// Bitmap bytes below which another thread costs more than it saves.
const PART: usize = 1024 * 1024;
const THREADS: usize = 8;
/// Takes about as long as the fastest level and leaves a third less to keep.
const LEVEL: u32 = 2;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Key {
    pub page: usize,
    /// The page's revision: a drawing made before its annotations changed is not reused.
    pub revision: u32,
    pub width: u32,
    pub height: u32,
}

/// A bitmap as raw deflate.
#[derive(PartialEq, Debug)]
pub struct Packed {
    pub bytes: Vec<u8>,
    /// Where each part ends, in `bytes` and in the bitmap.
    parts: Vec<(usize, usize)>,
}

#[derive(Default)]
pub struct BitmapCache {
    entries: VecDeque<(Key, Arc<Packed>)>,
    size: usize,
}

impl BitmapCache {
    pub fn get(&mut self, key: Key) -> Option<Arc<Packed>> {
        let i = self.entries.iter().position(|(k, _)| *k == key)?;
        let entry = self.entries.remove(i)?;
        let packed = entry.1.clone();
        self.entries.push_back(entry);
        Some(packed)
    }

    pub fn contains(&self, key: Key) -> bool {
        self.entries.iter().any(|(k, _)| *k == key)
    }

    pub fn put(&mut self, key: Key, packed: Arc<Packed>) {
        if packed.bytes.len() > BUDGET {
            return;
        }
        if let Some(i) = self.entries.iter().position(|(k, _)| *k == key)
            && let Some((_, old)) = self.entries.remove(i)
        {
            self.size -= old.bytes.len();
        }
        self.size += packed.bytes.len();
        self.entries.push_back((key, packed));
        while self.size > BUDGET {
            match self.entries.pop_front() {
                Some((_, v)) => self.size -= v.bytes.len(),
                None => break,
            }
        }
    }
}

fn threads() -> usize {
    std::thread::available_parallelism().map_or(1, |n| n.get()).min(THREADS)
}

pub fn pack(rgba: &[u8]) -> Result<Packed, String> {
    let count = (rgba.len() / PART).clamp(1, threads());
    let chunks: Vec<&[u8]> = if rgba.is_empty() { vec![rgba] } else { rgba.chunks(rgba.len().div_ceil(count)).collect() };
    let last = chunks.len() - 1;
    let deflated: Vec<Result<Vec<u8>, String>> = if last == 0 {
        vec![deflate(rgba, true)]
    } else {
        std::thread::scope(|scope| {
            let running: Vec<_> = chunks.iter().enumerate().map(|(i, chunk)| scope.spawn(move || deflate(chunk, i == last))).collect();
            running.into_iter().map(|r| r.join().unwrap_or_else(|_| Err("圧縮の処理が途中で止まりました".into()))).collect()
        })
    };
    let mut packed = Packed { bytes: Vec::new(), parts: Vec::with_capacity(chunks.len()) };
    let mut covered = 0;
    for (part, chunk) in deflated.into_iter().zip(&chunks) {
        packed.bytes.extend_from_slice(&part?);
        covered += chunk.len();
        packed.parts.push((packed.bytes.len(), covered));
    }
    packed.bytes.shrink_to_fit();
    Ok(packed)
}

/// One part as raw deflate: ended for good when `last`, otherwise on a byte
/// boundary so that the next part can follow it in the same stream.
fn deflate(input: &[u8], last: bool) -> Result<Vec<u8>, String> {
    let mut out = Vec::with_capacity(input.len() / 16 + 64);
    let mut encoder = Compress::new(Compression::new(LEVEL), false);
    let flush = if last { FlushCompress::Finish } else { FlushCompress::Sync };
    loop {
        let taken = encoder.total_in() as usize;
        let status = encoder.compress_vec(&input[taken..], &mut out, flush).map_err(|e| e.to_string())?;
        // A flush is complete once the encoder leaves room in the output.
        let flushed = !last && encoder.total_in() as usize == input.len() && out.len() < out.capacity();
        if status == Status::StreamEnd || flushed {
            return Ok(out);
        }
        out.reserve(out.capacity().max(4096));
    }
}

/// Inflates into `out`, which must have the size of the bitmap.
pub fn unpack_into(packed: &Packed, out: &mut [u8]) -> Result<(), String> {
    let whole = packed.parts.last().is_some_and(|&(_, end)| end == out.len())
        && std::thread::scope(|scope| {
            let mut rest = out;
            let mut running = Vec::with_capacity(packed.parts.len());
            let (mut from, mut covered) = (0, 0);
            for &(to, end) in &packed.parts {
                let (part, after) = std::mem::take(&mut rest).split_at_mut(end - covered);
                rest = after;
                let input = &packed.bytes[from..to];
                (from, covered) = (to, end);
                if packed.parts.len() == 1 {
                    return inflate(input, part);
                }
                running.push(scope.spawn(move || inflate(input, part)));
            }
            running.into_iter().all(|r| r.join().unwrap_or(false))
        });
    if whole { Ok(()) } else { Err("ページ画像を展開できませんでした".into()) }
}

pub fn unpack(packed: &Packed) -> Result<Vec<u8>, String> {
    let mut out = vec![0; packed.parts.last().map_or(0, |&(_, end)| end)];
    unpack_into(packed, &mut out)?;
    Ok(out)
}

/// Whether `input` inflates to exactly `out`.
fn inflate(input: &[u8], out: &mut [u8]) -> bool {
    let mut decoder = Decompress::new(false);
    loop {
        let (taken, made) = (decoder.total_in() as usize, decoder.total_out() as usize);
        let Ok(status) = decoder.decompress(&input[taken..], &mut out[made..], FlushDecompress::None) else {
            return false;
        };
        let (now_taken, now_made) = (decoder.total_in() as usize, decoder.total_out() as usize);
        if now_made == out.len() {
            return true;
        }
        if status == Status::StreamEnd || (now_taken == taken && now_made == made) {
            return false;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A page-like bitmap: white with a dark pixel now and then.
    fn page(width: usize, height: usize) -> Vec<u8> {
        let mut page = vec![255u8; width * height * 4];
        for i in (0..page.len()).step_by(4 * 37) {
            page[i] = (i / 7) as u8;
        }
        page
    }

    /// The whole stream through one decoder, as a web view reads it.
    fn inflate_as_one(packed: &Packed) -> Vec<u8> {
        let mut out = Vec::with_capacity(packed.parts.last().unwrap().1);
        let status = Decompress::new(false).decompress_vec(&packed.bytes, &mut out, FlushDecompress::Finish).unwrap();
        assert_eq!(status, Status::StreamEnd);
        out
    }

    #[test]
    fn round_trip_and_budget() {
        let page = page(100, 50);
        let packed = Arc::new(pack(&page).unwrap());
        assert!(packed.bytes.len() < page.len() / 4);
        assert_eq!(unpack(&packed).unwrap(), page);
        assert!(unpack_into(&packed, &mut vec![0; 100 * 49 * 4]).is_err());

        let mut cache = BitmapCache::default();
        let key = |page: usize| Key { page, revision: 0, width: 100, height: 50 };
        cache.put(key(0), packed.clone());
        assert_eq!(cache.get(key(0)).unwrap(), packed);
        assert!(cache.get(key(1)).is_none());
        assert!(cache.get(Key { revision: 1, ..key(0) }).is_none(), "a drawing before a change is not reused");
        let large = Arc::new(Packed { bytes: vec![0; BUDGET], parts: vec![(BUDGET, 0)] });
        cache.put(key(1), large);
        assert!(cache.get(key(0)).is_none());
    }

    #[test]
    fn parts_make_one_stream_and_inflate_on_their_own() {
        for (width, height) in [(1, 1), (640, 409), (1500, 1000), (1501, 999)] {
            let page = page(width, height);
            let packed = pack(&page).unwrap();
            assert_eq!(packed.parts.len(), (page.len() / PART).clamp(1, threads()));
            assert_eq!(packed.parts.last(), Some(&(packed.bytes.len(), page.len())));
            assert!(unpack(&packed).unwrap() == page, "{width} x {height} side by side");
            assert!(inflate_as_one(&packed) == page, "{width} x {height} as one stream");
        }
    }

    #[test]
    fn a_cut_part_is_not_taken_for_a_page() {
        let page = page(1500, 1000);
        let mut packed = pack(&page).unwrap();
        let middle = packed.parts[0].0 / 2;
        packed.bytes.truncate(middle);
        packed.parts = vec![(middle, page.len())];
        assert!(unpack(&packed).is_err());
    }
}
