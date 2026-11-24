//! Annotation model: reading PDF annotations, grouping them into comment
//! threads the way Acrobat presents them, and writing Acrobat-compatible
//! annotation dictionaries.

pub mod appearance;
pub mod date;
mod fields;
pub mod mark;
pub mod model;
pub mod read;
pub mod reshape;
pub mod thread;
pub mod write;

pub use date::PdfDate;
pub use model::{AnnotKey, Annotation, Quad, ReplyType, Subtype};
pub use read::read_all;
pub use thread::{Kind, Thread, build as build_threads, latest_state, off_page};
