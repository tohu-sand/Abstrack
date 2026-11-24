//! Appearance streams (/AP /N) for new annotations, drawn the way Acrobat
//! draws them. A viewer that does not build appearances itself, PDFium among
//! them, shows nothing without one.

use pdf_io::{Dictionary, Object, Stream};

/// A quadrilateral's corners: top-left, top-right, bottom-left, bottom-right.
pub type Corners = [(f64, f64); 4];

/// A form XObject with `content`, whose bounding box `bbox` is mapped by
/// `matrix` onto the annotation's rectangle.
pub fn form(bbox: [f64; 4], matrix: [f64; 6], resources: Dictionary, content: String) -> Stream {
    let mut dict = Dictionary::new();
    dict.set("Type", Object::Name(b"XObject".to_vec()));
    dict.set("Subtype", Object::Name(b"Form".to_vec()));
    dict.set("FormType", Object::Integer(1));
    dict.set("BBox", numbers(&bbox));
    dict.set("Matrix", numbers(&matrix));
    dict.set("Resources", Object::Dictionary(resources));
    Stream::new(dict, content.into_bytes()).with_compression(false)
}

/// The form that draws the highlight's quadrilaterals in page space:
/// `Form` in Acrobat's three nested forms.
pub fn highlight_quads(rect: [f64; 4], quads: &[Corners], color: [f64; 3]) -> Stream {
    let mut content = format!("{} rg\n", rgb(color));
    for q in quads {
        let [tl, tr, bl, br] = q.map(point);
        content.push_str(&format!("{bl} m {tl} l {tr} l {br} l f\n"));
    }
    form(rect, [1.0, 0.0, 0.0, 1.0, -rect[0], -rect[1]], proc_set(), content)
}

/// The transparency group around the quadrilaterals (`MWFOForm`), so that
/// where they overlap the colour is laid down once.
pub fn highlight_group(size: (f64, f64), quads_form: pdf_io::ObjectId) -> Stream {
    let mut xobjects = Dictionary::new();
    xobjects.set("Form", Object::Reference(quads_form));
    let mut resources = Dictionary::new();
    resources.set("XObject", Object::Dictionary(xobjects));
    let mut stream = form([0.0, 0.0, size.0, size.1], IDENTITY, resources, "/Form Do\n".to_string());
    let mut group = Dictionary::new();
    group.set("S", Object::Name(b"Transparency".to_vec()));
    stream.dict.set("Group", Object::Dictionary(group));
    stream
}

/// The highlight's appearance: the group laid over the page with the given
/// opacity, multiplying with what is below so that the text stays black.
pub fn highlight(size: (f64, f64), opacity: f64, group: pdf_io::ObjectId) -> Stream {
    let mut fade = Dictionary::new();
    fade.set("Type", Object::Name(b"ExtGState".to_vec()));
    fade.set("CA", Object::Real(opacity as f32));
    fade.set("ca", Object::Real(opacity as f32));
    fade.set("AIS", Object::Boolean(false));
    let mut multiply = Dictionary::new();
    multiply.set("Type", Object::Name(b"ExtGState".to_vec()));
    multiply.set("BM", Object::Name(b"Multiply".to_vec()));
    multiply.set("AIS", Object::Boolean(false));
    let mut states = Dictionary::new();
    states.set("R0", Object::Dictionary(fade));
    states.set("R1", Object::Dictionary(multiply));
    let mut xobjects = Dictionary::new();
    xobjects.set("MWFOForm", Object::Reference(group));
    let mut resources = proc_set();
    resources.set("ExtGState", Object::Dictionary(states));
    resources.set("XObject", Object::Dictionary(xobjects));
    form([0.0, 0.0, size.0, size.1], IDENTITY, resources, "/R0 gs /R1 gs /MWFOForm Do\n".to_string())
}

/// Strike-out (`at` = 3/7) and underline (`at` = 1/7): a line through each
/// quadrilateral at that fraction of its height, h/16 thick.
pub fn line_through(rect: [f64; 4], quads: &[Corners], color: [f64; 3], at: f64) -> Stream {
    let mut content = format!("{} RG\n", rgb(color));
    for &[tl, tr, bl, br] in quads {
        let height = ((tl.0 - bl.0).powi(2) + (tl.1 - bl.1).powi(2)).sqrt();
        let left = (bl.0 + (tl.0 - bl.0) * at, bl.1 + (tl.1 - bl.1) * at);
        let right = (br.0 + (tr.0 - br.0) * at, br.1 + (tr.1 - br.1) * at);
        content.push_str(&format!("{} w {} m {} l S\n", num(height / 16.0), point(left), point(right)));
    }
    form(rect, [1.0, 0.0, 0.0, 1.0, -rect[0], -rect[1]], proc_set(), content)
}

/// An insertion mark `w` wide and `h` high, drawn inside a margin of `rd`.
pub fn caret(w: f64, h: f64, rd: f64, color: [f64; 3]) -> Stream {
    let c = rgb(color);
    let (hw, hh) = (num(w / 2.0), num(h / 2.0));
    let content = format!(
        "{c} RG {rd} w {c} rg 0 0 m {hw} 0 {hw} {hh} {hw} {h} c {hw} {hh} {hw} 0 {w} 0 c h f S\n",
        rd = num(rd),
        h = num(h),
        w = num(w),
    );
    form([-rd, -rd, w + rd, h + rd], [1.0, 0.0, 0.0, 1.0, rd, rd], proc_set(), content)
}

/// A rectangle with a 1 pt line, inset by `rd` and half the line.
pub fn square(rect: [f64; 4], rd: f64, color: [f64; 3]) -> Stream {
    let inset = rd + 0.5;
    let content = format!(
        "{} RG 1 w {} {} {} {} re S\n",
        rgb(color),
        num(rect[0] + inset),
        num(rect[1] + inset),
        num(rect[2] - rect[0] - 2.0 * inset),
        num(rect[3] - rect[1] - 2.0 * inset),
    );
    form(rect, [1.0, 0.0, 0.0, 1.0, -rect[0], -rect[1]], proc_set(), content)
}

/// The icon of a note: a speech balloon, 24 by 24.
pub fn note_icon(color: [f64; 3]) -> Stream {
    let content = format!(
        "{} rg 0.2 0.2 0.2 RG 0.75 w\n\
         4 22 m 20 22 l 21.6 22 22.5 21.1 22.5 19.5 c 22.5 10.5 l 22.5 8.9 21.6 8 20 8 c\n\
         11 8 l 6 3 l 7 8 l 4 8 l 2.4 8 1.5 8.9 1.5 10.5 c 1.5 19.5 l 1.5 21.1 2.4 22 4 22 c h B\n\
         5.5 18 m 18.5 18 l 5.5 15 m 18.5 15 l 5.5 12 m 14 12 l S\n",
        rgb(color)
    );
    form([0.0, 0.0, 24.0, 24.0], IDENTITY, proc_set(), content)
}

const IDENTITY: [f64; 6] = [1.0, 0.0, 0.0, 1.0, 0.0, 0.0];

fn proc_set() -> Dictionary {
    let mut resources = Dictionary::new();
    resources.set("ProcSet", Object::Array(vec![Object::Name(b"PDF".to_vec())]));
    resources
}

pub fn numbers(values: &[f64]) -> Object {
    Object::Array(values.iter().map(|&v| Object::Real(v as f32)).collect())
}

/// A number for a content stream: up to four decimals, no trailing zeros.
pub fn num(v: f64) -> String {
    let text = format!("{:.4}", v);
    let text = text.trim_end_matches('0').trim_end_matches('.');
    if text == "-0" { "0".to_string() } else { text.to_string() }
}

fn point((x, y): (f64, f64)) -> String {
    format!("{} {}", num(x), num(y))
}

fn rgb(c: [f64; 3]) -> String {
    format!("{} {} {}", num(c[0]), num(c[1]), num(c[2]))
}
