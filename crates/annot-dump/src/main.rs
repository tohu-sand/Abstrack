//! annot-dump: lists the annotation dictionaries of PDF files.
//!
//! ```text
//! annot-dump [--ap] [--page N] FILE...
//! annot-dump --threads FILE...
//! annot-dump --revisions FILE...
//! annot-dump --obj N FILE
//! ```
//!
//! `--ap` also prints the decoded content of appearance streams.
//! `--threads` groups annotations into comment-list rows: replacement pairs,
//! replies and states under the annotation they belong to.
//! `--revisions` lists the objects each saved revision added, changed or removed.
//! `--obj` prints one indirect object (a stream with its decoded content).

mod print;
mod revisions;
mod threads;

use pdf_io::{AnnotRef, Object, PdfFile, XrefKind};
use print::{Printer, inline, name};
use std::process::ExitCode;

struct Options {
    show_streams: bool,
    threads: bool,
    revisions: bool,
    page: Option<usize>,
    object: Option<u32>,
    files: Vec<String>,
}

fn parse_args() -> Result<Options, String> {
    let mut opts = Options {
        show_streams: false,
        threads: false,
        revisions: false,
        page: None,
        object: None,
        files: Vec::new(),
    };
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--ap" => opts.show_streams = true,
            "--threads" => opts.threads = true,
            "--revisions" => opts.revisions = true,
            "--page" => {
                let v = args.next().ok_or("--page needs a number")?;
                opts.page = Some(v.parse().map_err(|_| format!("bad page number: {v}"))?);
            }
            "--obj" => {
                let v = args.next().ok_or("--obj needs an object number")?;
                opts.object = Some(v.parse().map_err(|_| format!("bad object number: {v}"))?);
            }
            "-h" | "--help" => return Err(String::new()),
            s if s.starts_with("--") => return Err(format!("unknown option: {s}")),
            _ => opts.files.push(arg),
        }
    }
    if opts.files.is_empty() {
        return Err(String::new());
    }
    Ok(opts)
}

fn main() -> ExitCode {
    let opts = match parse_args() {
        Ok(o) => o,
        Err(msg) => {
            if !msg.is_empty() {
                eprintln!("{msg}");
            }
            eprintln!("usage: annot-dump [--ap] [--page N] FILE...");
            eprintln!("       annot-dump --threads FILE...");
            eprintln!("       annot-dump --revisions FILE...");
            eprintln!("       annot-dump --obj N FILE");
            return ExitCode::from(2);
        }
    };
    let mut failed = false;
    for path in &opts.files {
        if let Err(e) = dump_file(path, &opts) {
            eprintln!("{path}: {e}");
            failed = true;
        }
    }
    if failed {
        ExitCode::FAILURE
    } else {
        ExitCode::SUCCESS
    }
}

fn dump_file(path: &str, opts: &Options) -> Result<(), String> {
    let bytes = std::fs::read(path).map_err(|e| e.to_string())?;
    if opts.revisions {
        println!("file: {path}");
        revisions::print_revisions(&bytes);
        return Ok(());
    }
    let file = PdfFile::open(bytes).map_err(|e| e.to_string())?;
    if let Some(number) = opts.object {
        return dump_object(&file, number);
    }
    if opts.threads {
        println!("file: {path}");
        threads::print_threads(&file);
        return Ok(());
    }
    let pages = file.page_ids();
    let total: usize = pages.iter().map(|&p| file.page_annots(p).len()).sum();
    println!("file: {path}");
    println!(
        "pages: {}  annotations: {}  xref: {}  version: {}",
        pages.len(),
        total,
        match file.xref_kind {
            XrefKind::Table => "table",
            XrefKind::Stream => "stream",
        },
        file.doc.version
    );
    if let Some(reason) = &file.read_only {
        println!("read-only: {reason:?}");
    }
    let printer = Printer {
        file: &file,
        show_streams: opts.show_streams,
    };
    for (index, &page_id) in pages.iter().enumerate() {
        let page_no = index + 1;
        if opts.page.is_some_and(|p| p != page_no) {
            continue;
        }
        let annots = file.page_annots(page_id);
        if annots.is_empty() {
            continue;
        }
        println!();
        println!(
            "== page {page_no}  ({} 0 R)  {}",
            page_id.0,
            page_geometry(&file, page_id)
        );
        for annot in &annots {
            dump_annot(&printer, &annots, annot.index, annot);
        }
    }
    Ok(())
}

fn page_geometry(file: &PdfFile, page_id: pdf_io::ObjectId) -> String {
    let Ok(page) = file.doc.get_dictionary(page_id) else {
        return String::new();
    };
    ["MediaBox", "CropBox", "Rotate", "UserUnit"]
        .iter()
        .filter_map(|k| {
            page.get(k.as_bytes())
                .ok()
                .map(|v| format!("/{k} {}", inline(v)))
        })
        .collect::<Vec<_>>()
        .join("  ")
}

fn dump_annot(printer: &Printer<'_>, annots: &[AnnotRef<'_>], index: usize, annot: &AnnotRef<'_>) {
    let subtype = annot
        .dict
        .get(b"Subtype")
        .ok()
        .and_then(|o| o.as_name().ok())
        .map(name)
        .unwrap_or_else(|| "?".into());
    let id = match annot.id {
        Some(id) => format!("{} {} obj", id.0, id.1),
        None => "inline".into(),
    };
    println!("-- [{index}] {id}  {subtype}");
    let mut out = String::new();
    printer.dict_block(annot.dict, 3, &mut out);
    print!("{out}");
    if let Ok(Object::Reference(popup)) = annot.dict.get(b"Popup")
        && let Ok(popup_dict) = printer.file.doc.get_dictionary(*popup)
        && !annots.iter().any(|a| a.id == Some(*popup))
    {
        println!("   % popup {} 0 R is not in /Annots:", popup.0);
        let mut out = String::new();
        printer.dict_block(popup_dict, 5, &mut out);
        print!("{out}");
    }
}

fn dump_object(file: &PdfFile, number: u32) -> Result<(), String> {
    let (&id, object) = file
        .doc
        .objects
        .range((number, 0)..=(number, u16::MAX))
        .next()
        .ok_or_else(|| format!("object {number} not found"))?;
    println!("{} {} obj", id.0, id.1);
    let printer = Printer {
        file,
        show_streams: true,
    };
    let mut out = String::new();
    match object {
        Object::Stream(stream) => {
            printer.dict_block(&stream.dict, 3, &mut out);
            print!("{out}");
            let content = stream
                .decompressed_content()
                .unwrap_or_else(|_| stream.content.clone());
            println!("stream ({} bytes decoded)", content.len());
            println!("{}", String::from_utf8_lossy(&content));
        }
        Object::Dictionary(dict) => {
            printer.dict_block(dict, 3, &mut out);
            print!("{out}");
        }
        other => println!("{}", inline(other)),
    }
    Ok(())
}
