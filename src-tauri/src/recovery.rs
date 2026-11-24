//! Keeping changes that are not saved yet safe from a crash. While the open
//! document has such changes, a copy of its file with the changes appended is
//! kept in the application's data folder, locked by the running program, and
//! removed once the changes are saved or given up. A copy that no running
//! program holds was left by a run that ended abruptly, and is offered back.

use crate::document::OpenDocument;
use crate::state::{AppState, lock, status};
use serde::{Deserialize, Serialize};
use std::fs::{File, OpenOptions, TryLockError};
use std::io::{Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::sync::mpsc::{Receiver, RecvTimeoutError, Sender, channel};
use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tauri::{AppHandle, Manager};

/// Changes coming close together are written once.
const QUIET: Duration = Duration::from_millis(500);

/// Written next to each copy.
#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Note {
    /// The file the changes belong to.
    pub path: PathBuf,
    /// The copy is this many bytes of that file, as it was read, followed by the changes.
    pub base_length: u64,
    /// When the copy was last written, as shown to the user.
    pub written: String,
}

/// A copy left behind, as offered to the user.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Offer {
    pub id: String,
    pub path: String,
    pub file_name: String,
    pub written: String,
}

struct Kept {
    generation: u64,
    /// The bytes the copy starts with: a save gives the document new ones.
    base: Arc<Vec<u8>>,
    /// The copy, locked for as long as this run keeps it.
    file: File,
    /// The copy and its note without their extensions.
    stem: PathBuf,
}

#[derive(Default)]
struct Keeper {
    kept: Option<Kept>,
    /// The program is ending: nothing is written any more.
    finished: bool,
}

enum Job {
    Keep {
        generation: u64,
        path: PathBuf,
        base: Arc<Vec<u8>>,
        changes: Arc<Vec<u8>>,
    },
    Forget,
}

pub struct Recovery {
    dir: Option<PathBuf>,
    jobs: Mutex<Sender<Job>>,
    keeper: Arc<Mutex<Keeper>>,
}

impl Recovery {
    pub fn start(app: &AppHandle, dir: Option<PathBuf>) -> Recovery {
        let (sender, receiver) = channel();
        let keeper = Arc::new(Mutex::new(Keeper::default()));
        if let Some(dir) = dir.clone() {
            let app = app.clone();
            let keeper = keeper.clone();
            let spawned = std::thread::Builder::new()
                .name("recovery".into())
                .spawn(move || run(&app, &dir, &receiver, &keeper));
            if let Err(e) = spawned {
                eprintln!("cannot start the recovery thread: {e}");
            }
        }
        Recovery {
            dir,
            jobs: Mutex::new(sender),
            keeper,
        }
    }

    /// After a change: keeps the document's changes (`changes`, appended to
    /// `base`), or removes the copy when there are none.
    pub fn keep(&self, doc: &OpenDocument, base: &Arc<Vec<u8>>, changes: Option<&Arc<Vec<u8>>>) {
        let job = match changes {
            Some(changes) => Job::Keep {
                generation: doc.generation,
                path: doc.path(),
                base: base.clone(),
                changes: changes.clone(),
            },
            None => Job::Forget,
        };
        // Without a data folder there is no thread to receive it; that was
        // reported at the start.
        let _ = lock(&self.jobs).send(job);
    }

    /// Removes the copy at once: its changes were given up.
    pub fn forget(&self) -> std::io::Result<()> {
        remove(lock(&self.keeper).kept.take())
    }

    /// At the end of the program, which asked before giving changes up.
    pub fn finish(&self) {
        let mut keeper = lock(&self.keeper);
        keeper.finished = true;
        // Nothing can be shown any more; a copy that stays is offered at the next start.
        let _ = remove(keeper.kept.take());
    }

    /// Copies left by runs that ended without removing them, newest first.
    /// Stray files of such runs are removed on the way.
    pub fn offers(&self) -> Vec<Offer> {
        let Some(dir) = &self.dir else { return Vec::new() };
        let Ok(entries) = std::fs::read_dir(dir) else { return Vec::new() };
        let mut found: Vec<(String, Note)> = Vec::new();
        for path in entries.flatten().map(|e| e.path()) {
            let stem = path.with_extension("");
            let Some(id) = stem.file_name().and_then(|n| n.to_str()).map(str::to_string) else { continue };
            if !valid_id(&id) || held(&stem) {
                continue;
            }
            let has = |ext: &str| stem.with_extension(ext).exists();
            match path.extension().and_then(|e| e.to_str()) {
                Some("json") if has("pdf") => found.extend(read_note(&path).map(|n| (id, n))),
                // A note without its copy, a copy whose first writing never
                // finished, a note being written: only tidied away, so one
                // that cannot be removed now is tried again next time.
                Some("json") => {
                    let _ = remove_files(&stem);
                }
                Some("pdf") if !has("json") => {
                    let _ = remove_files(&stem);
                }
                Some("tmp") => {
                    let _ = std::fs::remove_file(&path);
                }
                _ => {}
            }
        }
        found.sort_by(|a, b| b.0.cmp(&a.0));
        found
            .into_iter()
            .map(|(id, n)| Offer {
                id,
                file_name: n.path.file_name().map(|f| f.to_string_lossy().into_owned()).unwrap_or_default(),
                path: n.path.display().to_string(),
                written: n.written,
            })
            .collect()
    }

    /// The copy (without its extension) and the note of an offer, when it is
    /// still left behind.
    pub fn left(&self, id: &str) -> Result<(PathBuf, Note), String> {
        let dir = self.dir.as_ref().ok_or("復元用データの場所が分かりません")?;
        let stem = dir.join(id);
        if !valid_id(id) || held(&stem) {
            return Err("この復元用データは、起動中の別の Abstrack が使っています".into());
        }
        let note = read_note(&stem.with_extension("json")).ok_or("復元用データの情報を読めませんでした")?;
        Ok((stem, note))
    }

    /// Removes a copy left behind; the caller has asked the user first.
    pub fn discard(&self, id: &str) -> Result<(), String> {
        let (stem, _) = self.left(id)?;
        remove_files(&stem).map_err(|e| format!("復元用データ {} を削除できませんでした: {e}", stem.display()))
    }
}

fn run(app: &AppHandle, dir: &Path, jobs: &Receiver<Job>, keeper: &Mutex<Keeper>) {
    while let Ok(mut job) = jobs.recv() {
        loop {
            match jobs.recv_timeout(QUIET) {
                Ok(next) => job = next,
                Err(RecvTimeoutError::Timeout) => break,
                Err(RecvTimeoutError::Disconnected) => return,
            }
        }
        let mut keeper = lock(keeper);
        if keeper.finished {
            return;
        }
        let result = match job {
            Job::Keep { generation, path, base, changes } => {
                // The document may have been closed or replaced meanwhile.
                let current = lock(&app.state::<AppState>().document).as_ref().map(|d| d.generation);
                if current != Some(generation) {
                    continue;
                }
                write(dir, &mut keeper.kept, generation, &path, &base, &changes)
            }
            Job::Forget => remove(keeper.kept.take()),
        };
        if let Err(e) = result {
            status(app, format!("復元用データを書き込めないため、異常終了すると保存していない変更は戻せません: {e}"), true);
        }
    }
}

fn write(dir: &Path, kept: &mut Option<Kept>, generation: u64, path: &Path, base: &Arc<Vec<u8>>, changes: &[u8]) -> std::io::Result<()> {
    if kept.as_ref().is_some_and(|k| k.generation != generation || !Arc::ptr_eq(&k.base, base)) {
        remove(kept.take())?;
    }
    let k = match kept {
        Some(k) => k,
        None => {
            std::fs::create_dir_all(dir)?;
            let millis = SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| d.as_millis());
            let stem = dir.join(format!("{millis}-{}-{generation}", std::process::id()));
            let mut file = OpenOptions::new().read(true).write(true).create_new(true).open(stem.with_extension("pdf"))?;
            file.try_lock().map_err(|e| match e {
                TryLockError::WouldBlock => std::io::Error::other("復元用データを他のプログラムが使っています"),
                TryLockError::Error(e) => e,
            })?;
            file.write_all(base)?;
            kept.insert(Kept {
                generation,
                base: base.clone(),
                file,
                stem,
            })
        }
    };
    let length = k.base.len() as u64;
    k.file.set_len(length)?;
    k.file.seek(SeekFrom::Start(length))?;
    k.file.write_all(changes)?;
    k.file.sync_data()?;
    let note = Note {
        path: path.to_path_buf(),
        base_length: length,
        written: crate::clock::now().to_string(),
    };
    let temp = k.stem.with_extension("tmp");
    std::fs::write(&temp, serde_json::to_vec(&note).map_err(std::io::Error::other)?)?;
    std::fs::rename(&temp, k.stem.with_extension("json"))
}

fn remove(kept: Option<Kept>) -> std::io::Result<()> {
    match kept {
        Some(k) => {
            drop(k.file);
            remove_files(&k.stem)
        }
        None => Ok(()),
    }
}

/// The note goes first: a copy without its note is never offered.
pub fn remove_files(stem: &Path) -> std::io::Result<()> {
    for ext in ["json", "pdf"] {
        match std::fs::remove_file(stem.with_extension(ext)) {
            Err(e) if e.kind() != std::io::ErrorKind::NotFound => return Err(e),
            _ => {}
        }
    }
    Ok(())
}

/// Ids are made of digits and hyphens; anything else is not ours.
fn valid_id(id: &str) -> bool {
    !id.is_empty() && id.chars().all(|c| c.is_ascii_digit() || c == '-')
}

/// Whether a running program keeps this copy.
fn held(stem: &Path) -> bool {
    match OpenOptions::new().read(true).write(true).open(stem.with_extension("pdf")) {
        Ok(file) => matches!(file.try_lock(), Err(TryLockError::WouldBlock)),
        Err(_) => false,
    }
}

fn read_note(path: &Path) -> Option<Note> {
    serde_json::from_slice(&std::fs::read(path).ok()?).ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Read;

    /// Through the holding handle: on Windows the lock keeps other handles out.
    fn contents(kept: &Option<Kept>) -> Vec<u8> {
        let mut file = &kept.as_ref().unwrap().file;
        file.seek(SeekFrom::Start(0)).unwrap();
        let mut bytes = Vec::new();
        file.read_to_end(&mut bytes).unwrap();
        bytes
    }

    fn empty_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("abstrack-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        dir
    }

    fn recovery(dir: &Path) -> Recovery {
        Recovery {
            dir: Some(dir.to_path_buf()),
            jobs: Mutex::new(channel().0),
            keeper: Arc::default(),
        }
    }

    #[test]
    fn a_copy_is_kept_while_held_and_offered_once_left_behind() {
        let dir = empty_dir("recovery");
        let base = Arc::new(b"%PDF-1.7 original".to_vec());
        let path = Path::new("C:/papers/paper.pdf");
        let mut kept = None;
        write(&dir, &mut kept, 1, path, &base, b" first changes").unwrap();
        write(&dir, &mut kept, 1, path, &base, b" later").unwrap();
        let stem = kept.as_ref().unwrap().stem.clone();
        assert_eq!(contents(&kept), b"%PDF-1.7 original later");
        let note = read_note(&stem.with_extension("json")).unwrap();
        assert_eq!((note.path.as_path(), note.base_length), (path, base.len() as u64));

        let recovery = recovery(&dir);
        assert!(held(&stem));
        assert!(recovery.offers().is_empty(), "a copy this run holds is not offered");

        // The run ends without removing it.
        drop(kept.take().unwrap().file);
        let offers = recovery.offers();
        assert_eq!(offers.len(), 1);
        assert_eq!(offers[0].file_name, "paper.pdf");
        assert_eq!(recovery.left(&offers[0].id).unwrap().0, stem);

        recovery.discard(&offers[0].id).unwrap();
        assert!(recovery.offers().is_empty());
        assert!(!stem.with_extension("pdf").exists());
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn a_new_start_after_saving_replaces_the_copy() {
        let dir = empty_dir("resave");
        let path = Path::new("C:/papers/paper.pdf");
        let mut kept = None;
        write(&dir, &mut kept, 1, path, &Arc::new(b"before".to_vec()), b"+1").unwrap();
        let first = kept.as_ref().unwrap().stem.clone();
        write(&dir, &mut kept, 1, path, &Arc::new(b"before+1".to_vec()), b"+2").unwrap();
        let second = kept.as_ref().unwrap().stem.clone();
        assert_ne!(first, second);
        assert!(!first.with_extension("pdf").exists() && !first.with_extension("json").exists());
        assert_eq!(contents(&kept), b"before+1+2");
        remove(kept.take()).unwrap();
        assert_eq!(std::fs::read_dir(&dir).unwrap().count(), 0);
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn stray_files_are_tidied_and_foreign_names_left_alone() {
        let dir = empty_dir("stray");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("1-2-3.pdf"), b"half written").unwrap();
        std::fs::write(dir.join("4-5-6.json"), b"{}").unwrap();
        std::fs::write(dir.join("7-8-9.tmp"), b"{").unwrap();
        std::fs::write(dir.join("notes.txt"), b"someone else's").unwrap();
        assert!(recovery(&dir).offers().is_empty());
        let left: Vec<_> = std::fs::read_dir(&dir).unwrap().flatten().map(|e| e.file_name()).collect();
        assert_eq!(left, vec![std::ffi::OsString::from("notes.txt")]);
        assert!(recovery(&dir).left("../x").is_err());
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
