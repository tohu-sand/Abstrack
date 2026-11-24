//! A read-only view of several byte buffers laid end to end.
//!
//! The document PDFium draws is the original file followed by the update
//! that the next save would append, so the screen shows exactly what will be
//! written. The buffers are shared, not copied.

use std::io::{Read, Seek, SeekFrom};
use std::sync::Arc;

#[derive(Clone)]
pub struct ChainedBytes {
    parts: Vec<Arc<Vec<u8>>>,
    len: u64,
    pos: u64,
}

impl ChainedBytes {
    pub fn new(parts: Vec<Arc<Vec<u8>>>) -> ChainedBytes {
        let len = parts.iter().map(|p| p.len() as u64).sum();
        ChainedBytes { parts, len, pos: 0 }
    }

    pub fn len(&self) -> u64 {
        self.len
    }

    pub fn is_empty(&self) -> bool {
        self.len == 0
    }
}

impl Read for ChainedBytes {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        let mut written = 0;
        let mut start = 0u64;
        for part in &self.parts {
            let end = start + part.len() as u64;
            if written == buf.len() {
                break;
            }
            if self.pos < end {
                let offset = (self.pos - start) as usize;
                let n = (part.len() - offset).min(buf.len() - written);
                buf[written..written + n].copy_from_slice(&part[offset..offset + n]);
                written += n;
                self.pos += n as u64;
            }
            start = end;
        }
        Ok(written)
    }
}

impl Seek for ChainedBytes {
    fn seek(&mut self, to: SeekFrom) -> std::io::Result<u64> {
        let target = match to {
            SeekFrom::Start(n) => Some(n),
            SeekFrom::End(d) => self.len.checked_add_signed(d),
            SeekFrom::Current(d) => self.pos.checked_add_signed(d),
        };
        match target {
            Some(n) => {
                self.pos = n;
                Ok(n)
            }
            None => Err(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "seek before the start",
            )),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_across_parts() {
        let mut r = ChainedBytes::new(vec![
            Arc::new(b"abc".to_vec()),
            Arc::new(Vec::new()),
            Arc::new(b"defg".to_vec()),
        ]);
        assert_eq!(r.len(), 7);
        r.seek(SeekFrom::Start(2)).unwrap();
        let mut buf = [0u8; 4];
        assert_eq!(r.read(&mut buf).unwrap(), 4);
        assert_eq!(&buf, b"cdef");
        let mut rest = Vec::new();
        r.read_to_end(&mut rest).unwrap();
        assert_eq!(rest, b"g");
        r.seek(SeekFrom::End(-1)).unwrap();
        assert_eq!(r.read(&mut buf).unwrap(), 1);
        assert!(r.seek(SeekFrom::Current(-10)).is_err());
    }
}
