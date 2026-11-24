//! Noticing that the open file was changed by another program (Acrobat, a
//! synchronizing folder) or taken away. Its size and time are looked at every
//! so often; when they differ from what is known, its bytes are compared with
//! those the document was read from.

use crate::document::OpenDocument;
use crate::state::{AppState, lock};
use serde::Serialize;
use std::io::{self, BufRead, BufReader};
use std::path::Path;
use std::sync::Arc;
use std::time::{Duration, SystemTime};
use tauri::{AppHandle, Emitter, Manager};

const EVERY: Duration = Duration::from_millis(1500);

/// The size and time of a file, to tell cheaply that it may have changed.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Stamp {
    len: u64,
    modified: Option<SystemTime>,
}

impl Stamp {
    pub fn of(path: &Path) -> Option<Stamp> {
        let meta = std::fs::metadata(path).ok()?;
        Some(Stamp {
            len: meta.len(),
            modified: meta.modified().ok(),
        })
    }
}

/// How the file on disk stands against the bytes the document was read from.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Outside {
    #[default]
    Same,
    Changed,
    Missing,
}

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
struct Report {
    generation: u64,
    outside: Outside,
}

pub fn start(app: &AppHandle) {
    let app = app.clone();
    let spawned = std::thread::Builder::new().name("watch".into()).spawn(move || {
        loop {
            std::thread::sleep(EVERY);
            let doc = lock(&app.state::<AppState>().document).clone();
            if let Some(doc) = doc {
                look(&app, &doc);
            }
        }
    });
    if let Err(e) = spawned {
        eprintln!("cannot start the watch thread: {e}");
    }
}

fn unchanged(doc: &OpenDocument, path: &Path) -> bool {
    let now = Stamp::of(path);
    now.is_some() && now == *lock(&doc.known)
}

pub fn matches_file(path: &Path, expected: &[u8]) -> io::Result<bool> {
    matches_bytes(BufReader::with_capacity(64 * 1024, std::fs::File::open(path)?), expected)
}

/// Compares through EOF so a shorter or longer file cannot match its prefix.
fn matches_bytes(mut reader: impl BufRead, mut expected: &[u8]) -> io::Result<bool> {
    loop {
        let chunk = match reader.fill_buf() {
            Err(e) if e.kind() == io::ErrorKind::Interrupted => continue,
            other => other?,
        };
        if chunk.is_empty() {
            return Ok(expected.is_empty());
        }
        let Some(rest) = expected.strip_prefix(chunk) else { return Ok(false) };
        expected = rest;
        let len = chunk.len();
        reader.consume(len);
    }
}

fn look(app: &AppHandle, doc: &Arc<OpenDocument>) {
    if unchanged(doc, &doc.path()) {
        return;
    }
    let outside = {
        // A save holds the edit while it writes and then records the path and
        // the stamp, so under the same lock the file is before or after it.
        let edit = doc.edit();
        let path = doc.path();
        if unchanged(doc, &path) {
            return;
        }
        let now = Stamp::of(&path);
        let outside = match matches_file(&path, edit.file.bytes()) {
            Ok(true) => Outside::Same,
            Ok(false) => Outside::Changed,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Outside::Missing,
            // Being written by its program at this moment: look again next time.
            Err(_) => return,
        };
        *lock(&doc.known) = now;
        outside
    };
    set(app, doc, outside);
}

/// After the document was saved to `path`: that file is what it holds now.
/// The caller holds the edit.
pub fn settle(app: &AppHandle, doc: &OpenDocument, path: &Path) {
    *lock(&doc.known) = Stamp::of(path);
    set(app, doc, Outside::Same);
}

fn set(app: &AppHandle, doc: &OpenDocument, outside: Outside) {
    let before = std::mem::replace(&mut *lock(&doc.outside), outside);
    if before != outside {
        let report = Report {
            generation: doc.generation,
            outside,
        };
        if let Err(e) = app.emit("file-outside", report) {
            eprintln!("file-outside event failed: {e}");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compares_each_chunk_and_both_ends() {
        let same = |disk: &[u8], expected: &[u8]| matches_bytes(BufReader::with_capacity(3, disk), expected).unwrap();
        assert!(same(b"", b""));
        assert!(same(b"1234567", b"1234567"));
        assert!(!same(b"1234567", b"123456"));
        assert!(!same(b"123456", b"1234567"));
        assert!(!same(b"1234567", b"1234568"));
    }
}
