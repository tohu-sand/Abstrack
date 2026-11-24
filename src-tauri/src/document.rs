//! The opened document. Rust owns it; the window receives summaries.

use crate::edit::Edit;
use crate::state::lock;
use crate::texts::TextCache;
use crate::watch::{Outside, Stamp};
use pdf_io::{ObjectId, PageGeometry, PdfFile, ReadOnlyReason};
use serde::Serialize;
use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;
use std::sync::{Arc, Mutex, MutexGuard};

pub struct OpenDocument {
    pub generation: u64,
    /// Where it was opened from, or saved to last.
    pub path: Mutex<PathBuf>,
    pub pages: Vec<PageGeometry>,
    pub page_ids: Vec<ObjectId>,
    pub read_only: Option<ReadOnlyReason>,
    pub edit: Mutex<Edit>,
    pub texts: Mutex<TextCache>,
    pub bitmaps: Mutex<crate::bitmaps::BitmapCache>,
    /// Size and time of the file when it last matched the bytes read.
    pub known: Mutex<Option<Stamp>>,
    pub outside: Mutex<Outside>,
    /// The window has asked for a page of this document.
    pub asked: AtomicBool,
}

impl OpenDocument {
    /// `bytes` is the whole file, read beforehand so that no handle stays open
    /// and other programs can keep writing to the file; `known`, the file's
    /// stamp taken before reading it.
    pub fn parse(path: &Path, bytes: Arc<Vec<u8>>, known: Option<Stamp>, generation: u64) -> Result<OpenDocument, String> {
        let file = PdfFile::open_shared(bytes).map_err(|e| format!("{e}。ファイルが壊れていないか確認してください。"))?;
        let page_ids = file.page_ids();
        let pages = page_ids.iter().map(|&id| file.page_geometry(id)).collect::<Vec<_>>();
        let read_only = file.read_only.clone();
        let edit = Edit::new(file, page_ids.len());
        Ok(OpenDocument {
            generation,
            path: Mutex::new(path.to_path_buf()),
            pages,
            page_ids,
            read_only,
            edit: Mutex::new(edit),
            texts: Mutex::new(TextCache::default()),
            bitmaps: Mutex::default(),
            known: Mutex::new(known),
            outside: Mutex::new(Outside::Same),
            asked: AtomicBool::new(false),
        })
    }

    /// Displayed size of the first page and width of the widest, in points:
    /// with the window's size, they decide how large the first page is drawn.
    pub fn first_page_shape(&self) -> Option<([f64; 2], f64)> {
        let (width, height) = self.pages.first()?.display_size();
        let widest = self.pages.iter().map(|g| g.display_size().0).fold(0.0, f64::max);
        Some(([width, height], widest))
    }

    pub fn edit(&self) -> MutexGuard<'_, Edit> {
        lock(&self.edit)
    }

    pub fn path(&self) -> PathBuf {
        lock(&self.path).clone()
    }

    pub fn file_name(&self) -> String {
        self.path()
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default()
    }

    /// Pages that carry at least one listed comment.
    pub fn commented_pages(&self) -> Vec<usize> {
        let edit = self.edit();
        let mut pages: Vec<usize> = edit.threads.iter().map(|t| edit.annots[t.head].page).collect();
        pages.sort_unstable();
        pages.dedup();
        pages
    }

    pub fn info(&self) -> DocumentInfo {
        let edit = self.edit();
        DocumentInfo {
            generation: self.generation,
            path: self.path().display().to_string(),
            file_name: self.file_name(),
            read_only_reason: self.read_only.as_ref().map(|r| r.message().to_string()),
            comment_count: edit.threads.len(),
            dirty: edit.is_dirty(),
            outside: *lock(&self.outside),
            page_revisions: edit.page_revisions.clone(),
            pages: self
                .pages
                .iter()
                .map(|g| {
                    let (width, height) = g.display_size();
                    PageInfo {
                        width,
                        height,
                        visible: g.visible,
                        rotation: g.rotation,
                        user_unit: g.user_unit,
                    }
                })
                .collect(),
        }
    }

    pub fn outline(&self) -> Vec<OutlineItem> {
        match self.edit().file.doc.get_toc() {
            Ok(toc) => toc
                .toc
                .into_iter()
                .map(|t| OutlineItem {
                    level: t.level,
                    title: t.title,
                    page: t.page.saturating_sub(1),
                })
                .collect(),
            Err(_) => Vec::new(),
        }
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PageInfo {
    /// Displayed size in points.
    width: f64,
    height: f64,
    /// Visible area in user space.
    visible: [f64; 4],
    rotation: u16,
    user_unit: f64,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DocumentInfo {
    generation: u64,
    path: String,
    file_name: String,
    read_only_reason: Option<String>,
    comment_count: usize,
    /// Changes not saved yet.
    dirty: bool,
    outside: Outside,
    /// Per page, bumped when its annotations change.
    page_revisions: Vec<u32>,
    pages: Vec<PageInfo>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OutlineItem {
    level: usize,
    title: String,
    page: usize,
}
