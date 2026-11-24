//! One PDF annotation as read from the file, with the keys this application interprets.
//! Everything else stays in the file untouched; see `pdf_io`.

use crate::date::PdfDate;
use pdf_io::ObjectId;

/// Identifies an annotation within one opened file: its indirect object id,
/// or its position in a page's /Annots array when it is written inline.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum AnnotKey {
    Object(ObjectId),
    Inline { page: usize, index: usize },
}

impl std::fmt::Display for AnnotKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            AnnotKey::Object((num, 0)) => write!(f, "{num}"),
            AnnotKey::Object((num, generation)) => write!(f, "{num}.{generation}"),
            AnnotKey::Inline { page, index } => write!(f, "p{}[{index}]", page + 1),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Subtype {
    Text,
    Highlight,
    Underline,
    StrikeOut,
    Squiggly,
    Caret,
    Square,
    Circle,
    Line,
    Polygon,
    PolyLine,
    Ink,
    FreeText,
    Stamp,
    FileAttachment,
    Sound,
    Popup,
    Link,
    Widget,
    Other(String),
}

impl Subtype {
    pub fn from_name(name: &[u8]) -> Subtype {
        match name {
            b"Text" => Subtype::Text,
            b"Highlight" => Subtype::Highlight,
            b"Underline" => Subtype::Underline,
            b"StrikeOut" => Subtype::StrikeOut,
            b"Squiggly" => Subtype::Squiggly,
            b"Caret" => Subtype::Caret,
            b"Square" => Subtype::Square,
            b"Circle" => Subtype::Circle,
            b"Line" => Subtype::Line,
            b"Polygon" => Subtype::Polygon,
            b"PolyLine" => Subtype::PolyLine,
            b"Ink" => Subtype::Ink,
            b"FreeText" => Subtype::FreeText,
            b"Stamp" => Subtype::Stamp,
            b"FileAttachment" => Subtype::FileAttachment,
            b"Sound" => Subtype::Sound,
            b"Popup" => Subtype::Popup,
            b"Link" => Subtype::Link,
            b"Widget" => Subtype::Widget,
            other => Subtype::Other(String::from_utf8_lossy(other).into_owned()),
        }
    }

    pub fn name(&self) -> &str {
        match self {
            Subtype::Text => "Text",
            Subtype::Highlight => "Highlight",
            Subtype::Underline => "Underline",
            Subtype::StrikeOut => "StrikeOut",
            Subtype::Squiggly => "Squiggly",
            Subtype::Caret => "Caret",
            Subtype::Square => "Square",
            Subtype::Circle => "Circle",
            Subtype::Line => "Line",
            Subtype::Polygon => "Polygon",
            Subtype::PolyLine => "PolyLine",
            Subtype::Ink => "Ink",
            Subtype::FreeText => "FreeText",
            Subtype::Stamp => "Stamp",
            Subtype::FileAttachment => "FileAttachment",
            Subtype::Sound => "Sound",
            Subtype::Popup => "Popup",
            Subtype::Link => "Link",
            Subtype::Widget => "Widget",
            Subtype::Other(name) => name,
        }
    }

    /// Annotations that carry a comment and belong in the comment list
    /// (ISO 32000-1, 12.5.6.2 markup annotations).
    pub fn is_markup(&self) -> bool {
        !matches!(
            self,
            Subtype::Popup | Subtype::Link | Subtype::Widget | Subtype::Other(_)
        )
    }
}

/// /RT: how an annotation with /IRT relates to its target.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReplyType {
    Reply,
    Group,
}

/// A quadrilateral from /QuadPoints, in the order Acrobat writes the points:
/// top-left, top-right, bottom-left, bottom-right of the text run.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Quad {
    pub points: [(f64, f64); 4],
}

impl Quad {
    pub fn bounds(&self) -> [f64; 4] {
        let xs = self.points.map(|p| p.0);
        let ys = self.points.map(|p| p.1);
        [
            xs.iter().copied().fold(f64::INFINITY, f64::min),
            ys.iter().copied().fold(f64::INFINITY, f64::min),
            xs.iter().copied().fold(f64::NEG_INFINITY, f64::max),
            ys.iter().copied().fold(f64::NEG_INFINITY, f64::max),
        ]
    }
}

#[derive(Debug, Clone)]
pub struct Annotation {
    pub key: AnnotKey,
    /// 0-based page index.
    pub page: usize,
    /// Position in the page's /Annots array.
    pub index_on_page: usize,
    pub subtype: Subtype,
    /// /IT
    pub intent: Option<String>,
    /// /Rect normalized to [x0, y0, x1, y1] with x0 <= x1, y0 <= y1.
    pub rect: [f64; 4],
    pub quads: Vec<Quad>,
    /// /C as given: 0 (transparent), 1 (gray), 3 (RGB) or 4 (CMYK) components.
    pub color: Vec<f64>,
    /// /CA
    pub opacity: Option<f64>,
    /// /T
    pub author: Option<String>,
    /// /Contents
    pub contents: Option<String>,
    /// /RC, kept as written.
    pub rich_contents: Option<String>,
    /// /Subj
    pub subject: Option<String>,
    /// /NM
    pub name: Option<String>,
    /// /CreationDate
    pub created: Option<PdfDate>,
    /// /M (may also be a free-form string; only parseable dates are kept)
    pub modified: Option<PdfDate>,
    /// /IRT
    pub in_reply_to: Option<AnnotKey>,
    /// /RT, defaulting to Reply when /IRT is present.
    pub reply_type: Option<ReplyType>,
    /// /StateModel
    pub state_model: Option<String>,
    /// /State
    pub state: Option<String>,
    /// /Popup
    pub popup: Option<AnnotKey>,
    /// /Parent of a Popup
    pub parent: Option<AnnotKey>,
    /// /Name of a Text annotation (icon) or a Stamp
    pub icon: Option<String>,
}

/// Converts the components of /C to RGB. An empty array denotes no colour.
pub fn color_rgb(components: &[f64]) -> Option<[f64; 3]> {
    match *components {
        [g] => Some([g, g, g]),
        [r, g, b] => Some([r, g, b]),
        [c, m, y, k] => Some([(1.0 - c) * (1.0 - k), (1.0 - m) * (1.0 - k), (1.0 - y) * (1.0 - k)]),
        _ => None,
    }
}

impl Annotation {
    /// The latest known date of this annotation, for ordering replies and states.
    pub fn date(&self) -> Option<PdfDate> {
        match (self.modified, self.created) {
            (Some(m), Some(c)) => Some(m.max(c)),
            (m, c) => m.or(c),
        }
    }

    /// A Text annotation that records a state (/StateModel or /State present).
    /// Acrobat attaches it with /IRT to the annotation it describes, or to the
    /// previous state in the same model; one without a target is left over
    /// from a deleted annotation.
    pub fn is_state(&self) -> bool {
        self.subtype == Subtype::Text && (self.state_model.is_some() || self.state.is_some())
    }
}
