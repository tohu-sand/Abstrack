//! Page rendering and character geometry through PDFium.
//!
//! PDFium is used only to draw pages and to report where each character is.
//! Annotations are never written through it; the one change made to its copy
//! in memory hides the notes that Acrobat keeps off the page.

mod source;

pub use source::ChainedBytes;

use pdfium_render::prelude::*;
use std::path::Path;

#[derive(Debug)]
pub enum RenderError {
    Library(String),
    Document(String),
    Page(String),
}

impl std::fmt::Display for RenderError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            RenderError::Library(e) => write!(f, "PDFium を読み込めません: {e}"),
            RenderError::Document(e) => write!(f, "PDFium で文書を開けません: {e}"),
            RenderError::Page(e) => write!(f, "ページを処理できません: {e}"),
        }
    }
}

impl std::error::Error for RenderError {}

pub struct Engine {
    pdfium: Pdfium,
}

impl Engine {
    /// Loads the PDFium shared library from `dir` (pdfium.dll, libpdfium.dylib, ...).
    /// PDFium can be bound once per process.
    pub fn load(dir: &Path) -> Result<Engine, RenderError> {
        let path = Pdfium::pdfium_platform_library_name_at_path(dir);
        let bindings =
            Pdfium::bind_to_library(&path).map_err(|e| RenderError::Library(format!("{}: {e}", path.display())))?;
        Ok(Engine {
            pdfium: Pdfium::new(bindings),
        })
    }

    pub fn open(&self, bytes: ChainedBytes) -> Result<Document<'_>, RenderError> {
        let doc = self
            .pdfium
            .load_pdf_from_reader(bytes, None)
            .map_err(|e| RenderError::Document(e.to_string()))?;
        Ok(Document { doc })
    }
}

pub struct Document<'a> {
    doc: PdfDocument<'a>,
}

/// Where a page sits in its own user space, as PDFium draws it.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PageGeometry {
    /// Visible area (crop box limited to the media box), in user-space points.
    pub bbox: [f32; 4],
    /// Clockwise rotation applied when displaying: 0, 90, 180 or 270.
    pub rotation: u16,
}

impl PageGeometry {
    /// Displayed size in points, after rotation.
    pub fn display_size(&self) -> (f32, f32) {
        let w = self.bbox[2] - self.bbox[0];
        let h = self.bbox[3] - self.bbox[1];
        if self.rotation.is_multiple_of(180) { (w, h) } else { (h, w) }
    }
}

/// An RGBA bitmap, rows top to bottom.
pub struct Bitmap {
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
}

/// One character as PDFium reports it, in user-space points.
#[derive(Debug, Clone, PartialEq)]
pub struct CharBox {
    /// PDFium's index on the page.
    pub index: usize,
    pub ch: char,
    /// Box from the font's ascent and descent: [x0, y0, x1, y1].
    pub loose: [f32; 4],
    pub origin: (f32, f32),
    pub font_size: f32,
    /// Rotation of the glyph, in degrees.
    pub angle: f32,
    /// Inserted by PDFium (a space or line break that is not in the content stream).
    pub generated: bool,
}

/// How pages are drawn: annotations from their appearance streams, on white.
/// PDFium's direct drawing call (not the one through a matrix) places the page
/// at whole pixels, so that a region drawn with an offset has exactly the
/// pixels of the whole page there. Without a form in the file the form layer
/// draws nothing; with one, fields show over their widgets as the file has them.
fn drawing(width: i32, height: i32) -> PdfRenderConfig {
    PdfRenderConfig::new()
        .set_fixed_size(width, height)
        .render_annotations(true)
        .render_form_data(true)
        .set_reverse_byte_order(true)
        .clear_before_rendering(false)
}

/// PDFium draws into the white RGBA buffer that will be handed to the caller.
fn draw(page: &PdfPage<'_>, width: i32, height: i32, config: &PdfRenderConfig) -> Result<Vec<u8>, RenderError> {
    let len = width.checked_mul(4).and_then(|stride| stride.checked_mul(height))
        .filter(|&len| len > 0).ok_or_else(|| RenderError::Page("invalid bitmap size".into()))?;
    let mut rgba = vec![0; len as usize];
    whiten(&mut rgba);
    draw_over(page, width, height, config, &mut rgba)?;
    Ok(rgba)
}

/// Bitmap bytes below which one thread makes a bitmap white as fast as several.
const SPLIT: usize = 8 * 1024 * 1024;

/// Makes a bitmap white. Fresh memory is first touched here, which takes
/// longer than writing to it; a large bitmap is shared out between threads.
fn whiten(rgba: &mut [u8]) {
    if rgba.len() < SPLIT {
        return rgba.fill(255);
    }
    let part = rgba.len().div_ceil(4);
    std::thread::scope(|scope| {
        for chunk in rgba.chunks_mut(part) {
            scope.spawn(move || chunk.fill(255));
        }
    });
}

/// PDFium draws over what `rgba` holds: `width` x `height` pixels, no more and no less.
fn draw_over(page: &PdfPage<'_>, width: i32, height: i32, config: &PdfRenderConfig, rgba: &mut [u8]) -> Result<(), RenderError> {
    let len = width.checked_mul(4).and_then(|stride| stride.checked_mul(height)).filter(|&len| len > 0);
    if len.map(|len| len as usize) != Some(rgba.len()) {
        return Err(RenderError::Page("invalid bitmap size".into()));
    }
    let mut bitmap = PdfBitmap::from_bytes(width, height, PdfBitmapFormat::BGRA, rgba)
        .map_err(|e| RenderError::Page(e.to_string()))?;
    page.render_into_bitmap_with_config(&mut bitmap, config).map_err(|e| RenderError::Page(e.to_string()))
}

/// Hides the notes at these positions in the page's /Annots: replies and
/// states, which Acrobat shows only in the comment list. The flag is set in
/// PDFium's copy in memory and stays until the document is opened again. A
/// position that does not hold a note is drawn as usual.
fn hide_notes(page: &PdfPage<'_>, positions: &[usize]) -> Result<(), RenderError> {
    let annotations = page.annotations();
    for &position in positions {
        let Ok(mut note) = annotations.get(position) else {
            continue;
        };
        if note.annotation_type() == PdfPageAnnotationType::Text && !note.is_hidden() {
            note.set_is_hidden(true).map_err(|e| RenderError::Page(e.to_string()))?;
        }
    }
    Ok(())
}

impl Document<'_> {
    pub fn page_count(&self) -> usize {
        self.doc.pages().len() as usize
    }

    fn page(&self, index: usize) -> Result<PdfPage<'_>, RenderError> {
        let index = PdfPageIndex::try_from(index).map_err(|_| RenderError::Page(format!("page {index}")))?;
        self.doc
            .pages()
            .get(index)
            .map_err(|e| RenderError::Page(e.to_string()))
    }

    pub fn geometry(&self, index: usize) -> Result<PageGeometry, RenderError> {
        let page = self.page(index)?;
        Self::page_geometry(&page)
    }

    fn page_geometry(page: &PdfPage<'_>) -> Result<PageGeometry, RenderError> {
        let rect = page
            .boundaries()
            .bounding()
            .map(|b| b.bounds)
            .map_err(|e| RenderError::Page(e.to_string()))?;
        let rotation = match page.rotation() {
            Ok(PdfPageRenderRotation::Degrees90) => 90,
            Ok(PdfPageRenderRotation::Degrees180) => 180,
            Ok(PdfPageRenderRotation::Degrees270) => 270,
            _ => 0,
        };
        Ok(PageGeometry {
            bbox: [
                rect.left().value,
                rect.bottom().value,
                rect.right().value,
                rect.top().value,
            ],
            rotation,
        })
    }

    /// Renders the whole visible page scaled to a bitmap of `width` x `height`
    /// pixels (the displayed, rotated size), annotations included (from their
    /// appearance streams) except the notes at the positions `leave_out` in
    /// the page's /Annots, which stay hidden in later calls on this document
    /// (see `hide_notes`).
    pub fn render(&self, index: usize, width: u32, height: u32, leave_out: &[usize]) -> Result<Bitmap, RenderError> {
        let page = self.page(index)?;
        hide_notes(&page, leave_out)?;
        let w = i32::try_from(width).map_err(|_| RenderError::Page("bitmap too wide".into()))?;
        let h = i32::try_from(height).map_err(|_| RenderError::Page("bitmap too tall".into()))?;
        let rgba = draw(&page, w, h, &drawing(w, h))?;
        Ok(Bitmap { width, height, rgba })
    }

    /// As `render`, into `rgba`: memory of the caller's, such as a buffer
    /// shared with another process, that needs no copy afterwards. `rgba` must
    /// hold exactly `width` x `height` x 4 bytes; any other length is an error.
    pub fn render_into(&self, index: usize, width: u32, height: u32, leave_out: &[usize], rgba: &mut [u8]) -> Result<(), RenderError> {
        let page = self.page(index)?;
        hide_notes(&page, leave_out)?;
        let w = i32::try_from(width).map_err(|_| RenderError::Page("bitmap too wide".into()))?;
        let h = i32::try_from(height).map_err(|_| RenderError::Page("bitmap too tall".into()))?;
        whiten(rgba);
        draw_over(&page, w, h, &drawing(w, h), rgba)
    }

    /// Renders the part `[x, y, x + w, y + h)` of the page drawn at `width` x
    /// `height` pixels, counted from the top left: exactly the pixels the whole
    /// drawing has there, so that only where annotations changed needs drawing
    /// again. A glyph cut by the edge of a drawing comes out slightly
    /// differently, so a margin wider than the text is drawn around the region
    /// and left out. An empty region, or one reaching past the drawing, is an error.
    pub fn render_region(
        &self,
        index: usize,
        size: (u32, u32),
        region: [u32; 4],
        leave_out: &[usize],
    ) -> Result<Bitmap, RenderError> {
        let (width, height) = size;
        let [x, y, w, h] = region;
        if w == 0 || h == 0 || x.checked_add(w).is_none_or(|r| r > width) || y.checked_add(h).is_none_or(|b| b > height) {
            return Err(RenderError::Page(format!("region {x},{y} {w}x{h} outside {width}x{height}")));
        }
        let margin = (width / 25).max(8);
        let (left, top) = (x.saturating_sub(margin), y.saturating_sub(margin));
        let (right, bottom) = ((x + w).saturating_add(margin).min(width), (y + h).saturating_add(margin).min(height));
        let fits = |v: u32| i32::try_from(v).map_err(|_| RenderError::Page("bitmap too large".into()));
        let config = drawing(fits(width)?, fits(height)?).set_origin(-fits(left)?, -fits(top)?);
        let (dw, dh) = (right - left, bottom - top);
        let page = self.page(index)?;
        hide_notes(&page, leave_out)?;
        let drawn = draw(&page, fits(dw)?, fits(dh)?, &config)?;
        if (dw, dh) == (w, h) {
            return Ok(Bitmap { width: w, height: h, rgba: drawn });
        }
        let mut rgba = Vec::with_capacity((w * h * 4) as usize);
        for row in 0..h {
            let at = (((y - top + row) * dw + (x - left)) * 4) as usize;
            rgba.extend_from_slice(&drawn[at..at + (w * 4) as usize]);
        }
        Ok(Bitmap { width: w, height: h, rgba })
    }

    pub fn chars(&self, index: usize) -> Result<Vec<CharBox>, RenderError> {
        let page = self.page(index)?;
        Self::page_chars(&page)
    }

    pub fn chars_with_geometry(&self, index: usize) -> Result<(Vec<CharBox>, PageGeometry), RenderError> {
        let page = self.page(index)?;
        let geometry = Self::page_geometry(&page)?;
        Ok((Self::page_chars(&page)?, geometry))
    }

    fn page_chars(page: &PdfPage<'_>) -> Result<Vec<CharBox>, RenderError> {
        let text = page.text().map_err(|e| RenderError::Page(e.to_string()))?;
        let chars = text.chars();
        let mut out = Vec::with_capacity(chars.len());
        for c in chars.iter() {
            let Some(ch) = c.unicode_char() else {
                continue;
            };
            let rect = |r: Result<PdfRect, PdfiumError>| {
                r.map(|r| [r.left().value, r.bottom().value, r.right().value, r.top().value])
                    .unwrap_or([0.0; 4])
            };
            out.push(CharBox {
                index: c.index(),
                ch,
                loose: rect(c.loose_bounds()),
                origin: c
                    .origin()
                    .map(|(x, y)| (x.value, y.value))
                    .unwrap_or((0.0, 0.0)),
                font_size: c.scaled_font_size().value,
                angle: c.angle_degrees().unwrap_or(0.0),
                generated: c.is_generated().unwrap_or(false),
            });
        }
        Ok(out)
    }
}
