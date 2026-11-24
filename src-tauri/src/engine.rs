//! One thread owns PDFium. Page drawing for the window goes first; text
//! extraction for the comment list and search waits behind it.

use render::{Bitmap, ChainedBytes, Document, Engine};
use std::collections::{HashMap, HashSet, VecDeque};
use std::path::PathBuf;
use std::sync::mpsc::{Receiver, Sender, channel};
use std::sync::{Arc, Condvar, Mutex};
use textmodel::{InputChar, MarginBands, PageEdges, PageFrame, Rect, SourceChar, TextModel, inputs_from_source, sample_pages};

pub type Reply<T> = Sender<Result<T, String>>;

/// For each page, the positions in its /Annots of the notes not drawn on it
/// (replies and states, which Acrobat shows only in the comment list).
pub type OffPage = HashMap<usize, Vec<usize>>;

enum Job {
    Open {
        generation: u64,
        bytes: ChainedBytes,
        reply: Reply<usize>,
    },
    Render {
        generation: u64,
        page: usize,
        width: u32,
        height: u32,
        /// Only this part of the page drawn at `width` x `height`: x, y, w, h.
        region: Option<[u32; 4]>,
        reply: Reply<Bitmap>,
    },
    /// A whole page drawn into memory of the caller's.
    RenderInto {
        generation: u64,
        page: usize,
        width: u32,
        height: u32,
        target: Target,
        reply: Reply<()>,
    },
    Text {
        generation: u64,
        page: usize,
        reply: Reply<TextModel>,
    },
    /// The same document with changes appended: drawing shows them from now on.
    Reload {
        generation: u64,
        bytes: ChainedBytes,
        off_page: OffPage,
        reply: Reply<()>,
    },
    /// The notes of the document just opened that are not drawn.
    OffPage {
        generation: u64,
        off_page: OffPage,
        reply: Reply<()>,
    },
    /// Reads one of the pages from which the running headers and footers of
    /// the document are learnt. Queued when the document opens, ahead of any
    /// text extraction, one page at a time so that drawing can go between.
    Learn {
        generation: u64,
        page: usize,
    },
    Close,
}

/// Memory a page is drawn into in place, lent for one job.
struct Target {
    start: *mut u8,
    len: usize,
}

// SAFETY: `render_into` holds the exclusive borrow the pointer was made from
// until the job has answered or has been dropped, and nothing else reads the
// pointer.
unsafe impl Send for Target {}

struct Current<'a> {
    generation: u64,
    doc: Document<'a>,
    off_page: OffPage,
    edges: Vec<PageEdges>,
    learning: usize,
    bands: MarginBands,
}

#[derive(Default)]
struct Queues {
    urgent: VecDeque<Job>,
    background: VecDeque<Job>,
}

#[derive(Clone)]
pub struct EngineHandle {
    queues: Arc<(Mutex<Queues>, Condvar)>,
    /// Pages the window currently shows or is about to show.
    wanted: Arc<Mutex<HashSet<usize>>>,
}

impl EngineHandle {
    pub fn start(library_dir: Option<PathBuf>) -> EngineHandle {
        let handle = EngineHandle {
            queues: Arc::new((Mutex::new(Queues::default()), Condvar::new())),
            wanted: Arc::new(Mutex::new(HashSet::new())),
        };
        let worker = handle.clone();
        std::thread::Builder::new()
            .name("pdfium".into())
            .spawn(move || worker.run(library_dir))
            .expect("start the PDFium thread");
        handle
    }

    fn push(&self, job: Job, urgent: bool) {
        let (lock, ready) = &*self.queues;
        let mut q = lock.lock().unwrap_or_else(|p| p.into_inner());
        if urgent {
            q.urgent.push_back(job);
        } else {
            q.background.push_back(job);
        }
        ready.notify_one();
    }

    /// Puts a job before every waiting one: changes to the document must not
    /// wait for drawing asked for earlier (pages drawn after it show the change).
    fn push_first(&self, job: Job) {
        let (lock, ready) = &*self.queues;
        lock.lock().unwrap_or_else(|p| p.into_inner()).urgent.push_front(job);
        ready.notify_one();
    }

    pub fn open(&self, generation: u64, bytes: ChainedBytes) -> Result<usize, String> {
        let (reply, rx) = channel();
        self.push(Job::Open { generation, bytes, reply }, true);
        rx.recv().map_err(|e| e.to_string())?
    }

    pub fn close(&self) {
        self.push(Job::Close, true);
    }

    /// Replaces the open document by the same one with changes appended,
    /// ahead of any drawing still waiting and of any asked for afterwards.
    /// Whether PDFium could read it comes on the receiver.
    pub fn reload(&self, generation: u64, bytes: ChainedBytes, off_page: OffPage) -> Receiver<Result<(), String>> {
        let (reply, rx) = channel();
        self.push_first(Job::Reload { generation, bytes, off_page, reply });
        rx
    }

    /// Which notes of the document just opened are not drawn, ahead of any drawing.
    pub fn set_off_page(&self, generation: u64, off_page: OffPage) -> Result<(), String> {
        let (reply, rx) = channel();
        self.push_first(Job::OffPage { generation, off_page, reply });
        rx.recv().map_err(|e| e.to_string())?
    }

    pub fn set_wanted(&self, pages: impl IntoIterator<Item = usize>) {
        let mut wanted = self.wanted.lock().unwrap_or_else(|p| p.into_inner());
        wanted.clear();
        wanted.extend(pages);
    }

    pub fn wants_page(&self, page: usize) -> bool {
        self.wanted.lock().unwrap_or_else(|p| p.into_inner()).contains(&page)
    }

    /// Draws a page; `urgent` puts it before text extraction and prefetching.
    pub fn render(&self, generation: u64, page: usize, width: u32, height: u32, urgent: bool) -> Result<Bitmap, String> {
        self.draw(generation, page, (width, height), None, urgent)
    }

    /// Draws part of a page drawn at `size`, where its annotations changed.
    pub fn render_region(&self, generation: u64, page: usize, size: (u32, u32), region: [u32; 4]) -> Result<Bitmap, String> {
        self.draw(generation, page, size, Some(region), true)
    }

    /// Draws a page into `rgba`, `width` x `height` pixels, ahead of text
    /// extraction and prefetching.
    pub fn render_into(&self, generation: u64, page: usize, width: u32, height: u32, rgba: &mut [u8]) -> Result<(), String> {
        let (reply, rx) = channel();
        let target = Target { start: rgba.as_mut_ptr(), len: rgba.len() };
        self.push(Job::RenderInto { generation, page, width, height, target, reply }, true);
        // Waiting here keeps `rgba` borrowed for as long as the job can write to it.
        rx.recv().map_err(|e| e.to_string())?
    }

    fn draw(&self, generation: u64, page: usize, size: (u32, u32), region: Option<[u32; 4]>, urgent: bool) -> Result<Bitmap, String> {
        let (reply, rx) = channel();
        let (width, height) = size;
        self.push(
            Job::Render {
                generation,
                page,
                width,
                height,
                region,
                reply,
            },
            urgent,
        );
        rx.recv().map_err(|e| e.to_string())?
    }

    pub fn text_model(&self, generation: u64, page: usize) -> Result<TextModel, String> {
        let (reply, rx) = channel();
        self.push(Job::Text { generation, page, reply }, false);
        rx.recv().map_err(|e| e.to_string())?
    }

    fn next(&self) -> Job {
        let (lock, ready) = &*self.queues;
        let mut q = lock.lock().unwrap_or_else(|p| p.into_inner());
        loop {
            if let Some(job) = q.urgent.pop_front().or_else(|| q.background.pop_front()) {
                return job;
            }
            q = ready.wait(q).unwrap_or_else(|p| p.into_inner());
        }
    }

    fn run(&self, library_dir: Option<PathBuf>) {
        let engine = match library_dir {
            Some(dir) => Engine::load(&dir).map_err(|e| e.to_string()),
            None => Err("PDFium のライブラリが見つかりません。アプリを入れ直してください。".to_string()),
        };
        let mut current: Option<Current<'_>> = None;
        // `send` fails only once the receiver is dropped: no one waits for
        // that answer any more, so the error is ignored below.
        loop {
            match self.next() {
                Job::Open {
                    generation,
                    bytes,
                    reply,
                } => {
                    current = None;
                    let result = engine.as_ref().map_err(Clone::clone).and_then(|e| {
                        e.open(bytes).map_err(|e| e.to_string()).map(|doc| {
                            let pages = doc.page_count();
                            let samples = sample_pages(pages);
                            current = Some(Current {
                                generation,
                                doc,
                                off_page: OffPage::new(),
                                edges: Vec::with_capacity(samples.len()),
                                learning: samples.len(),
                                bands: MarginBands::default(),
                            });
                            for page in samples {
                                self.push(Job::Learn { generation, page }, false);
                            }
                            pages
                        })
                    });
                    let _ = reply.send(result);
                }
                Job::Close => current = None,
                Job::Reload {
                    generation,
                    bytes,
                    off_page,
                    reply,
                } => {
                    let result = match (current.as_mut(), engine.as_ref()) {
                        (Some(c), Ok(e)) if c.generation == generation => e
                            .open(bytes)
                            .map(|doc| {
                                c.doc = doc;
                                c.off_page = off_page;
                            })
                            .map_err(|e| e.to_string()),
                        (_, Err(e)) => Err(e.clone()),
                        _ => Err(STALE.to_string()),
                    };
                    let _ = reply.send(result);
                }
                Job::OffPage {
                    generation,
                    off_page,
                    reply,
                } => {
                    let result = match current.as_mut() {
                        Some(c) if c.generation == generation => {
                            c.off_page = off_page;
                            Ok(())
                        }
                        _ => Err(STALE.to_string()),
                    };
                    let _ = reply.send(result);
                }
                Job::Learn { generation, page } => {
                    if let Some(c) = current.as_mut().filter(|c| c.generation == generation) {
                        // A page whose text cannot be read adds nothing to learn from.
                        if let Ok((input, frame)) = page_input(&c.doc, page) {
                            c.edges.push(PageEdges::of(&input, frame));
                        }
                        c.learning -= 1;
                        if c.learning == 0 {
                            c.bands = MarginBands::learn(&c.edges);
                            c.edges = Vec::new();
                        }
                    }
                }
                Job::Render {
                    generation,
                    page,
                    width,
                    height,
                    region,
                    reply,
                } => {
                    let still_wanted = self.wants_page(page);
                    let result = match &current {
                        _ if !still_wanted => Err(SKIPPED.to_string()),
                        Some(c) if c.generation == generation => {
                            let off = c.off_page.get(&page).map_or(&[][..], Vec::as_slice);
                            match region {
                                Some(r) => c.doc.render_region(page, (width, height), r, off),
                                None => c.doc.render(page, width, height, off),
                            }
                            .map_err(|e| e.to_string())
                        }
                        _ => Err(STALE.to_string()),
                    };
                    let _ = reply.send(result);
                }
                Job::RenderInto {
                    generation,
                    page,
                    width,
                    height,
                    target,
                    reply,
                } => {
                    let still_wanted = self.wants_page(page);
                    let result = match &current {
                        _ if !still_wanted => Err(SKIPPED.to_string()),
                        Some(c) if c.generation == generation => {
                            let off = c.off_page.get(&page).map_or(&[][..], Vec::as_slice);
                            // SAFETY: the memory is borrowed by `render_into`, which waits for this answer.
                            let rgba = unsafe { std::slice::from_raw_parts_mut(target.start, target.len) };
                            c.doc.render_into(page, width, height, off, rgba).map_err(|e| e.to_string())
                        }
                        _ => Err(STALE.to_string()),
                    };
                    let _ = reply.send(result);
                }
                Job::Text {
                    generation,
                    page,
                    reply,
                } => {
                    let result = match &current {
                        Some(c) if c.generation == generation => page_input(&c.doc, page)
                            .map(|(input, frame)| TextModel::build_with(&input, frame, &c.bands)),
                        _ => Err(STALE.to_string()),
                    };
                    let _ = reply.send(result);
                }
            }
        }
    }
}

/// The page left the view before its turn came.
pub const SKIPPED: &str = "skipped";
/// Another document was opened meanwhile.
pub const STALE: &str = "stale";

fn page_input(doc: &Document<'_>, page: usize) -> Result<(Vec<InputChar>, PageFrame), String> {
    let (chars, geometry) = doc.chars_with_geometry(page).map_err(|e| e.to_string())?;
    let input = inputs_from_source(chars.into_iter().map(|c| SourceChar {
        ch: c.ch,
        bbox: c.loose,
        size: c.font_size,
        angle: c.angle,
        origin: c.origin,
        index: c.index,
        generated: c.generated,
    }));
    let frame = PageFrame {
        bbox: Rect::from_array(geometry.bbox),
        rotation: geometry.rotation,
    };
    Ok((input, frame))
}

/// Where the PDFium shared library is: next to the executable when installed,
/// in vendor/ during development.
pub fn library_dir() -> Option<PathBuf> {
    let name = if cfg!(windows) {
        "pdfium.dll"
    } else if cfg!(target_os = "macos") {
        "libpdfium.dylib"
    } else {
        "libpdfium.so"
    };
    let exe_dir = std::env::current_exe().ok()?.parent()?.to_path_buf();
    let mut candidates = vec![exe_dir.clone(), exe_dir.join("../Frameworks"), exe_dir.join("../Resources")];
    if cfg!(debug_assertions) {
        let vendor = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../vendor/pdfium");
        let target = match (std::env::consts::OS, std::env::consts::ARCH) {
            ("windows", "aarch64") => "win-arm64/bin",
            ("windows", _) => "win-x64/bin",
            ("macos", "aarch64") => "mac-arm64/lib",
            ("macos", _) => "mac-x64/lib",
            _ => "lib",
        };
        candidates.push(vendor.join(target));
    }
    candidates.into_iter().find(|dir| dir.join(name).is_file())
}
