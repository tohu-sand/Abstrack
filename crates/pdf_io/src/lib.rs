//! Reading a PDF for annotation work and appending incremental updates to it.

mod geometry;
mod tail;
mod text;
mod update;

pub use lopdf;
pub use lopdf::{Dictionary, Object, ObjectId, Stream, StringFormat};
pub use geometry::PageGeometry;
pub use text::{decode_text, encode_pdfdoc, encode_text, text_object, text_string};
pub use update::{Changes, Increment, UpdateError, increment};

use lopdf::{Document, LoadOptions};
use std::sync::Arc;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum XrefKind {
    Table,
    Stream,
}

/// Why a file is opened for viewing only.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ReadOnlyReason {
    Encrypted,
    Signed,
    DamagedXref,
}

impl ReadOnlyReason {
    pub fn message(&self) -> &'static str {
        match self {
            ReadOnlyReason::Encrypted => {
                "暗号化されたPDFのため閲覧専用で開きました。コメントを入れるには、暗号化を解除したPDFを用意してください。"
            }
            ReadOnlyReason::Signed => {
                "電子署名付きのPDFのため閲覧専用で開きました。コメントを保存すると署名の検証結果が変わるおそれがあります。"
            }
            ReadOnlyReason::DamagedXref => {
                "ファイルの一部が壊れているため閲覧専用で開きました。Acrobat などで保存し直したPDFを開いてください。"
            }
        }
    }
}

#[derive(Debug)]
pub enum OpenError {
    Parse(lopdf::Error),
    NotPdf,
}

impl std::fmt::Display for OpenError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            OpenError::Parse(e) => write!(f, "PDFとして読み込めません: {e}"),
            OpenError::NotPdf => write!(f, "PDFファイルではありません"),
        }
    }
}

impl std::error::Error for OpenError {}

/// A parsed PDF together with the exact bytes it was read from.
pub struct PdfFile {
    bytes: Arc<Vec<u8>>,
    pub doc: Document,
    /// Offset of the newest cross-reference section, as given by the last `startxref`.
    pub startxref: Option<u64>,
    pub xref_kind: XrefKind,
    pub read_only: Option<ReadOnlyReason>,
    /// Streams whose content was not kept (see [`PdfFile::open_shared`]).
    /// They must never be written back.
    pub hollow: std::collections::HashSet<ObjectId>,
}

/// Image, font and page content streams larger than this lose their content
/// in [`PdfFile::open_shared`].
const HOLLOW_ABOVE: usize = 4096;

impl PdfFile {
    /// Opens a file keeping every object whole (for inspecting a file).
    pub fn open(bytes: Vec<u8>) -> Result<PdfFile, OpenError> {
        PdfFile::load(Arc::new(bytes), false)
    }

    /// Opens bytes that other readers (the renderer) share, for annotation
    /// work. The content of large streams that annotation work never reads
    /// (images, fonts, page contents) is dropped right after parsing; this
    /// halves the memory a scanned or figure-heavy file takes. Object and
    /// cross-reference streams and form XObjects (which annotation
    /// appearances are) stay whole.
    pub fn open_shared(bytes: Arc<Vec<u8>>) -> Result<PdfFile, OpenError> {
        PdfFile::load(bytes, true)
    }

    fn load(bytes: Arc<Vec<u8>>, hollow_streams: bool) -> Result<PdfFile, OpenError> {
        if !tail::has_pdf_header(&bytes) {
            return Err(OpenError::NotPdf);
        }
        let options = LoadOptions {
            filter: hollow_streams.then_some(hollow_large_streams as lopdf::FilterFunc),
            ..LoadOptions::default()
        };
        let mut doc = Document::load_mem_with_options(&bytes, options).map_err(OpenError::Parse)?;
        let mut hollow: std::collections::HashSet<ObjectId> = std::collections::HashSet::new();
        if hollow_streams {
            hollow.extend(
                doc.objects
                    .iter()
                    .filter(|(_, o)| matches!(o, Object::Stream(s) if s.content.is_empty() && image_or_font(&s.dict)))
                    .map(|(&id, _)| id),
            );
            hollow_page_contents(&mut doc, &mut hollow);
        }
        let startxref = tail::find_startxref(&bytes);
        let xref_kind = match startxref {
            Some(offset) if tail::xref_table_at(&bytes, offset) => XrefKind::Table,
            Some(_) => XrefKind::Stream,
            None => XrefKind::Table,
        };
        let read_only = if doc.trailer.has(b"Encrypt") {
            Some(ReadOnlyReason::Encrypted)
        } else if has_signature(&doc) {
            Some(ReadOnlyReason::Signed)
        } else if doc.xref_recovery.is_some() || startxref.is_none() {
            Some(ReadOnlyReason::DamagedXref)
        } else {
            None
        };
        Ok(PdfFile {
            bytes,
            doc,
            startxref,
            xref_kind,
            read_only,
            hollow,
        })
    }

    pub fn bytes(&self) -> &Arc<Vec<u8>> {
        &self.bytes
    }

    /// Page object ids in page order (index 0 is page 1).
    pub fn page_ids(&self) -> Vec<ObjectId> {
        self.doc.get_pages().into_values().collect()
    }

    pub fn resolve<'a>(&'a self, obj: &'a Object) -> &'a Object {
        resolve(&self.doc, obj)
    }

    /// Entries of a page's /Annots array. Each entry is either an indirect
    /// reference (the usual case) or a dictionary written inline.
    pub fn page_annots(&self, page_id: ObjectId) -> Vec<AnnotRef<'_>> {
        let Ok(page) = self.doc.get_dictionary(page_id) else {
            return Vec::new();
        };
        let Ok(annots) = page.get(b"Annots") else {
            return Vec::new();
        };
        let Ok(list) = self.resolve(annots).as_array() else {
            return Vec::new();
        };
        list.iter()
            .enumerate()
            .filter_map(|(index, entry)| match entry {
                Object::Reference(id) => match self.doc.get_object(*id) {
                    Ok(Object::Dictionary(dict)) => Some(AnnotRef {
                        id: Some(*id),
                        dict,
                        index,
                    }),
                    _ => None,
                },
                Object::Dictionary(dict) => Some(AnnotRef { id: None, dict, index }),
                _ => None,
            })
            .collect()
    }
}

/// Images and embedded fonts: the bulk of most files, and never read for annotations.
fn image_or_font(dict: &Dictionary) -> bool {
    let subtype = dict.get(b"Subtype").ok().and_then(|o| o.as_name().ok());
    matches!(subtype, Some(b"Image" | b"Type1C" | b"CIDFontType0C" | b"OpenType"))
        || dict.has(b"Length1")
        || dict.has(b"Length2")
        || dict.has(b"Length3")
}

/// Load filter: keeps every object, but empties large image and font streams.
/// Called both with objects in place (the returned copy is then ignored) and
/// with objects taken out of object streams (the returned copy is kept);
/// streams only occur in the first case.
fn hollow_large_streams(id: ObjectId, object: &mut Object) -> Option<(ObjectId, Object)> {
    if let Object::Stream(stream) = object {
        if stream.content.len() > HOLLOW_ABOVE && image_or_font(&stream.dict) {
            stream.content = Vec::new();
        }
        return Some((id, Object::Null));
    }
    Some((id, object.clone()))
}

/// Empties the content streams of the pages, which only the renderer reads.
fn hollow_page_contents(doc: &mut Document, hollow: &mut std::collections::HashSet<ObjectId>) {
    let mut ids = Vec::new();
    for page_id in doc.get_pages().into_values() {
        let Ok(page) = doc.get_dictionary(page_id) else { continue };
        match page.get(b"Contents") {
            Ok(Object::Reference(id)) => {
                ids.push(*id);
                if let Ok(Object::Array(items)) = doc.get_object(*id) {
                    ids.extend(items.iter().filter_map(|o| o.as_reference().ok()));
                }
            }
            Ok(Object::Array(items)) => ids.extend(items.iter().filter_map(|o| o.as_reference().ok())),
            _ => {}
        }
    }
    for id in ids {
        if let Ok(Object::Stream(stream)) = doc.get_object_mut(id)
            && stream.content.len() > HOLLOW_ABOVE
        {
            stream.content = Vec::new();
            stream.content.shrink_to_fit();
            hollow.insert(id);
        }
    }
}

#[derive(Clone, Copy)]
pub struct AnnotRef<'a> {
    pub id: Option<ObjectId>,
    pub dict: &'a Dictionary,
    /// Position in the page's /Annots array, counting entries that are not
    /// annotations, as PDFium counts them.
    pub index: usize,
}

/// Follows indirect references. A missing object, or a chain of references
/// too long to be anything but a cycle, resolves to Null.
pub fn resolve<'a>(doc: &'a Document, mut obj: &'a Object) -> &'a Object {
    for _ in 0..32 {
        match obj {
            Object::Reference(id) => match doc.get_object(*id) {
                Ok(next) => obj = next,
                Err(_) => return &Object::Null,
            },
            _ => return obj,
        }
    }
    &Object::Null
}

/// Any of: /Perms in the catalog (certification or usage rights), the
/// SignaturesExist bit of /SigFlags, or a signature field with a value.
fn has_signature(doc: &Document) -> bool {
    let Ok(root) = doc.trailer.get(b"Root") else {
        return false;
    };
    let Ok(catalog) = resolve(doc, root).as_dict() else {
        return false;
    };
    if catalog.has(b"Perms") {
        return true;
    }
    let Ok(acroform) = catalog.get(b"AcroForm") else {
        return false;
    };
    let Ok(acroform) = resolve(doc, acroform).as_dict() else {
        return false;
    };
    if let Ok(flags) = acroform.get(b"SigFlags").map(|o| resolve(doc, o))
        && let Ok(flags) = flags.as_i64()
        && flags & 1 != 0
    {
        return true;
    }
    let Ok(fields) = acroform.get(b"Fields") else {
        return false;
    };
    let Ok(fields) = resolve(doc, fields).as_array() else {
        return false;
    };
    let mut stack: Vec<&Object> = fields.iter().collect();
    let mut visited = 0;
    while let Some(field) = stack.pop() {
        visited += 1;
        if visited > 100_000 {
            break;
        }
        let Ok(field) = resolve(doc, field).as_dict() else {
            continue;
        };
        let is_sig = field
            .get(b"FT")
            .ok()
            .and_then(|o| resolve(doc, o).as_name().ok())
            == Some(b"Sig".as_slice());
        if is_sig && field.has(b"V") {
            return true;
        }
        if let Ok(kids) = field.get(b"Kids")
            && let Ok(kids) = resolve(doc, kids).as_array()
        {
            stack.extend(kids.iter());
        }
    }
    false
}
