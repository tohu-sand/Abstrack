//! Application settings in the OS configuration directory. Comments and their
//! states never go here; they live in the PDF.

use crate::placement::Placement;
use annot::write::{Colors, DEFAULT_COLORS};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

const RECENT_LIMIT: usize = 10;

/// The kinds of comment whose color can be chosen, by the window's names.
pub const COLOR_KINDS: [&str; 7] = ["highlight", "replace", "strikeOut", "underline", "insert", "note", "area"];

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct Settings {
    pub author: String,
    pub recent_files: Vec<String>,
    pub list_visible: bool,
    pub list_width: u32,
    pub list_font_size: u32,
    pub window: Option<Placement>,
    /// Colors chosen for new comments, `#rrggbb` by kind; the other kinds
    /// keep their defaults.
    pub comment_colors: BTreeMap<String, String>,
    /// How large the window last asked for the first page of a document.
    pub first_draw: Option<FirstDraw>,
}

/// The size a first page was drawn at, and what decided it besides the
/// window: a document with the same page sizes is drawn at that size while
/// the window is still starting.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct FirstDraw {
    /// Displayed size of the first page and width of the widest page, in points.
    pub page: [f64; 2],
    pub widest: f64,
    /// Pixels.
    pub width: u32,
    pub height: u32,
}

impl Default for Settings {
    fn default() -> Settings {
        Settings {
            author: String::new(),
            recent_files: Vec::new(),
            list_visible: true,
            list_width: 360,
            list_font_size: 13,
            window: None,
            comment_colors: BTreeMap::new(),
            first_draw: None,
        }
    }
}

impl Settings {
    /// Missing file: defaults. Unreadable file: defaults and the reason, so the
    /// caller can tell the user instead of silently forgetting their settings.
    pub fn load(path: &Path) -> (Settings, Option<String>) {
        match std::fs::read(path) {
            Ok(bytes) => match serde_json::from_slice::<Settings>(&bytes) {
                Ok(s) => (s.sanitized(), None),
                Err(e) => (
                    Settings::default(),
                    Some(format!("設定ファイル {} を読み込めなかったので、既定の設定で起動しました: {e}", path.display())),
                ),
            },
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => (Settings::default(), None),
            Err(e) => (
                Settings::default(),
                Some(format!("設定ファイル {} を開けなかったので、既定の設定で起動しました: {e}", path.display())),
            ),
        }
    }

    pub fn save(&self, path: &Path) -> std::io::Result<()> {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        let tmp = temp_path(path);
        std::fs::write(&tmp, serde_json::to_vec_pretty(self).map_err(std::io::Error::other)?)?;
        std::fs::rename(&tmp, path)
    }

    /// The colors new comments get: the chosen ones, and the defaults for the rest.
    pub fn colors(&self) -> Colors {
        let mut colors = DEFAULT_COLORS;
        for (kind, hex) in &self.comment_colors {
            if let (Some(slot), Some(rgb)) = (color_of(&mut colors, kind), from_hex(hex)) {
                *slot = rgb;
            }
        }
        colors
    }

    pub fn remember(&mut self, file: &Path) {
        let entry = normalized(&file.display().to_string());
        self.recent_files.retain(|f| !same_path(f, &entry));
        self.recent_files.insert(0, entry);
        self.recent_files.truncate(RECENT_LIMIT);
    }

    fn sanitized(mut self) -> Settings {
        self.list_width = self.list_width.clamp(200, 900);
        self.list_font_size = self.list_font_size.clamp(10, 24);
        let mut recent: Vec<String> = Vec::new();
        for f in self.recent_files.iter().map(|f| normalized(f)) {
            if !recent.iter().any(|r| same_path(r, &f)) {
                recent.push(f);
            }
        }
        recent.truncate(RECENT_LIMIT);
        self.recent_files = recent;
        self.comment_colors.retain(|kind, hex| COLOR_KINDS.contains(&kind.as_str()) && from_hex(hex).is_some());
        self
    }
}

/// The color of `kind`, one of [`COLOR_KINDS`], in `colors`.
pub fn color_of<'a>(colors: &'a mut Colors, kind: &str) -> Option<&'a mut [f64; 3]> {
    match kind {
        "highlight" => Some(&mut colors.highlight),
        "replace" => Some(&mut colors.replace),
        "strikeOut" => Some(&mut colors.strike_out),
        "underline" => Some(&mut colors.underline),
        "insert" => Some(&mut colors.insert),
        "note" => Some(&mut colors.note),
        "area" => Some(&mut colors.area),
        _ => None,
    }
}

/// `#rrggbb` as RGB from 0 to 1.
pub fn from_hex(hex: &str) -> Option<[f64; 3]> {
    let digits = hex.strip_prefix('#').filter(|d| d.len() == 6 && d.chars().all(|c| c.is_ascii_hexdigit()))?;
    let n = u32::from_str_radix(digits, 16).ok()?;
    Some([(n >> 16) as f64 / 255.0, ((n >> 8) & 0xff) as f64 / 255.0, (n & 0xff) as f64 / 255.0])
}

/// RGB from 0 to 1 as `#rrggbb`.
pub fn to_hex(rgb: [f64; 3]) -> String {
    let byte = |v: f64| (v.clamp(0.0, 1.0) * 255.0).round() as u8;
    format!("#{:02x}{:02x}{:02x}", byte(rgb[0]), byte(rgb[1]), byte(rgb[2]))
}

/// A Windows drive letter in upper case, as Explorer shows it.
fn normalized(path: &str) -> String {
    let mut chars = path.chars();
    match (chars.next(), chars.next()) {
        (Some(d), Some(':')) if d.is_ascii_alphabetic() => format!("{}{}", d.to_ascii_uppercase(), &path[1..]),
        _ => path.to_string(),
    }
}

/// Windows and macOS file systems ignore case by default.
fn same_path(a: &str, b: &str) -> bool {
    if cfg!(any(windows, target_os = "macos")) {
        a.to_lowercase() == b.to_lowercase()
    } else {
        a == b
    }
}

fn temp_path(path: &Path) -> PathBuf {
    let mut name = path.file_name().map(|n| n.to_os_string()).unwrap_or_default();
    name.push(".tmp");
    path.with_file_name(name)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip_and_recent_order() {
        let dir = std::env::temp_dir().join(format!("abstrack-settings-{}", std::process::id()));
        let path = dir.join("settings.json");
        let mut s = Settings::default();
        for i in 0..12 {
            s.remember(Path::new(&format!("C:/papers/{i}.pdf")));
        }
        s.remember(Path::new("C:/papers/3.pdf"));
        assert_eq!(s.recent_files.len(), RECENT_LIMIT);
        assert_eq!(s.recent_files[0], "C:/papers/3.pdf");
        if cfg!(windows) {
            s.remember(Path::new("c:/Papers/3.pdf"));
            assert_eq!(s.recent_files[0], "C:/Papers/3.pdf");
            assert_eq!(s.recent_files.iter().filter(|f| f.to_lowercase() == "c:/papers/3.pdf").count(), 1);
        }
        s.save(&path).unwrap();
        let (loaded, problem) = Settings::load(&path);
        assert!(problem.is_none());
        assert_eq!(loaded, s);
        std::fs::write(&path, b"{ broken").unwrap();
        let (fallback, problem) = Settings::load(&path);
        assert_eq!(fallback, Settings::default());
        assert!(problem.is_some());
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn chosen_colors_over_the_defaults() {
        let mut s = Settings::default();
        for (kind, hex) in [("insert", "#00FF80"), ("plain", "#123456"), ("note", "yellow"), ("area", "#12345")] {
            s.comment_colors.insert(kind.into(), hex.into());
        }
        let s = s.sanitized();
        assert_eq!(s.comment_colors.keys().collect::<Vec<_>>(), ["insert"]);
        let colors = s.colors();
        assert_eq!(colors.insert, [0.0, 1.0, 128.0 / 255.0]);
        assert_eq!(colors.note, DEFAULT_COLORS.note);
        assert_eq!(to_hex(DEFAULT_COLORS.strike_out), "#f86464");
        assert_eq!(from_hex("#f86464").map(to_hex).as_deref(), Some("#f86464"));
    }
}
