//! Text models of the open document's pages, built on demand and kept for the
//! comment list, search and selection.

use crate::document::OpenDocument;
use crate::engine::EngineHandle;
use crate::state::lock;
use serde::Serialize;
use std::collections::HashMap;
use std::sync::Arc;
use textmodel::{SearchQuery, SearchText, TextModel};

/// Pages kept; older ones are built again when needed (a few milliseconds each).
const KEEP: usize = 16;
const SEARCH_BUDGET: usize = 4 * 1024 * 1024;

#[derive(Default)]
pub struct TextCache {
    /// Most recently used last.
    pages: Vec<(usize, Arc<TextModel>)>,
    /// Kept when a model is evicted or searched. Annotation edits leave page text intact.
    searchable: HashMap<usize, Arc<SearchText>>,
    search_bytes: usize,
}

impl TextCache {
    fn take(&mut self, page: usize) -> Option<Arc<TextModel>> {
        let i = self.pages.iter().position(|(p, _)| *p == page)?;
        let entry = self.pages.remove(i);
        let model = entry.1.clone();
        self.pages.push(entry);
        Some(model)
    }

    fn keep(&mut self, page: usize, model: Arc<TextModel>) -> Option<(usize, Arc<TextModel>)> {
        self.pages.retain(|(p, _)| *p != page);
        self.pages.push((page, model));
        (self.pages.len() > KEEP).then(|| self.pages.remove(0))
    }

    fn can_index(&self, page: usize, model: &TextModel) -> bool {
        model.chars.len() + std::mem::size_of::<SearchText>() <= SEARCH_BUDGET - self.search_bytes
            && !self.searchable.contains_key(&page)
    }

    fn keep_search_text(&mut self, page: usize, text: Arc<SearchText>) {
        let size = text.byte_len() + std::mem::size_of::<SearchText>();
        if size <= SEARCH_BUDGET - self.search_bytes
            && let std::collections::hash_map::Entry::Vacant(slot) = self.searchable.entry(page)
        {
            slot.insert(text);
            self.search_bytes += size;
        }
    }
}

pub fn page_text(engine: &EngineHandle, doc: &OpenDocument, page: usize) -> Result<Arc<TextModel>, String> {
    read_page(engine, doc, page, true)
}

/// An offscreen page used to place comments need not displace a page being selected.
pub fn list_page_text(engine: &EngineHandle, doc: &OpenDocument, page: usize) -> Result<Arc<TextModel>, String> {
    read_page(engine, doc, page, engine.wants_page(page))
}

fn read_page(
    engine: &EngineHandle,
    doc: &OpenDocument,
    page: usize,
    keep_model: bool,
) -> Result<Arc<TextModel>, String> {
    if let Some(model) = lock(&doc.texts).take(page) {
        return Ok(model);
    }
    let model = Arc::new(engine.text_model(doc.generation, page)?);
    let discarded = if keep_model { lock(&doc.texts).keep(page, model.clone()) } else { Some((page, model.clone())) };
    if let Some((p, unused)) = discarded {
        let can_index = lock(&doc.texts).can_index(p, &unused);
        if can_index {
            let text = Arc::new(SearchText::of(&unused));
            lock(&doc.texts).keep_search_text(p, text);
        }
    }
    Ok(model)
}

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct SearchHit {
    pub page: usize,
    /// One rectangle per line the match covers, user space.
    pub rects: Vec<[f32; 4]>,
    /// Carets before the first and after the last character found, in the
    /// page's reading order: closing the search selects them.
    pub start: usize,
    pub end: usize,
}

pub fn search_text(engine: &EngineHandle, doc: &OpenDocument, page: usize) -> Result<Arc<SearchText>, String> {
    if let Some(text) = lock(&doc.texts).searchable.get(&page).cloned() {
        return Ok(text);
    }
    let model = page_text(engine, doc, page)?;
    let text = Arc::new(SearchText::of(&model));
    lock(&doc.texts).keep_search_text(page, text.clone());
    Ok(text)
}

pub fn hits_on_page(engine: &EngineHandle, doc: &OpenDocument, page: usize, query: &SearchQuery) -> Result<Vec<SearchHit>, String> {
    let ranges = search_text(engine, doc, page)?.ranges(query);
    if ranges.is_empty() {
        return Ok(Vec::new());
    }
    let model = page_text(engine, doc, page)?;
    Ok(ranges
        .into_iter()
        .map(|range| SearchHit {
            page,
            rects: model.range_rects(range.start, range.end),
            start: range.start,
            end: range.end,
        })
        .collect())
}
