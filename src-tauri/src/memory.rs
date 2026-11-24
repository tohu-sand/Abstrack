//! Giving memory back while the window is not in use.
//!
//! The window is idle when it is not focused, is minimized, or has had no
//! input for a while (the page reports that). Entering idle tells WebView2 to
//! aim for low memory use (ICoreWebView2_19 MemoryUsageTargetLevel), which
//! makes it drop caches and trim its page process, and trims this process's
//! working set too. When the window is not even in front (unfocused or
//! minimized), the working sets of the WebView2 processes started for this
//! window are trimmed as well; Windows brings pages back when they are used.
//! WebView2 trims when the level changes, so after work done while idle (a
//! page drawn, a document opened) the change is made again. Any use of the
//! window returns to normal at once.

use crate::state::status;
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};
use tauri::{AppHandle, Manager, Runtime, WebviewWindow};

#[derive(Default)]
pub struct Governor {
    state: Mutex<State>,
    /// Memory grew since the last trim.
    grew: AtomicBool,
    /// The program has been running long enough to have its first page up.
    /// Before that nothing is trimmed: a window that starts behind another
    /// would take back at once what it is loading.
    settled: AtomicBool,
}

struct State {
    /// The page reported no input for its idle period.
    page_idle: bool,
    /// The level WebView2 was last told; `None` when telling it failed.
    low: Option<bool>,
    last_trim: Option<Instant>,
}

impl Default for State {
    fn default() -> State {
        // A web view starts at the normal level: telling it so while it loads
        // its first page only holds the window's thread up.
        State { page_idle: false, low: Some(false), last_trim: None }
    }
}

/// After work while idle, how long to wait for it to settle before trimming.
const SETTLE: Duration = Duration::from_secs(3);

impl Governor {
    pub fn note_growth(&self) {
        self.grew.store(true, Ordering::SeqCst);
    }

    pub fn set_page_idle(&self, idle: bool) {
        self.state.lock().unwrap_or_else(|p| p.into_inner()).page_idle = idle;
    }
}

/// Re-evaluates idleness from the window and the page, and applies it.
pub fn update<R: Runtime>(app: &AppHandle, window: &WebviewWindow<R>) {
    let governor = app.state::<Governor>();
    if !governor.settled.load(Ordering::SeqCst) {
        return;
    }
    let in_use = in_front(window) && !window.is_minimized().unwrap_or(false);
    let mut s = governor.state.lock().unwrap_or_else(|p| p.into_inner());
    let idle = !in_use || s.page_idle;
    let background = !in_use;
    let grew = governor.grew.load(Ordering::SeqCst);
    let settled = s.last_trim.is_none_or(|t| t.elapsed() >= SETTLE);
    let trim_again = idle && s.low == Some(true) && grew && settled;
    if s.low == Some(idle) && !trim_again {
        return;
    }
    s.low = Some(idle);
    if idle {
        s.last_trim = Some(Instant::now());
        governor.grew.store(false, Ordering::SeqCst);
    }
    drop(s);
    apply(app, window, idle, trim_again, background);
}

/// Whether the window is the one being worked in. On Windows the keyboard is
/// in the web view, a child window, for as long as the page is used, and the
/// window itself then reports that it is not focused; being the foreground
/// window is what tells. That also holds while its menu bar is open.
fn in_front<R: Runtime>(window: &WebviewWindow<R>) -> bool {
    #[cfg(windows)]
    {
        let Ok(hwnd) = window.hwnd() else {
            return true;
        };
        // SAFETY: plain query of which window is in front.
        unsafe { windows::Win32::UI::WindowsAndMessaging::GetForegroundWindow() == hwnd }
    }
    #[cfg(not(windows))]
    {
        window.is_focused().unwrap_or(true)
    }
}

fn apply<R: Runtime>(app: &AppHandle, window: &WebviewWindow<R>, low: bool, retrigger: bool, background: bool) {
    let handle = app.clone();
    let result = window.with_webview(move |webview| {
        #[cfg(windows)]
        {
            let outcome = (|| {
                if retrigger {
                    windows_impl::set_level(&webview, false)?;
                }
                windows_impl::set_level(&webview, low)?;
                if low {
                    windows_impl::trim_working_sets(background);
                }
                Ok::<(), windows_core::Error>(())
            })();
            if let Err(e) = outcome {
                status(&handle, format!("メモリ使用量の指定を WebView2 に伝えられませんでした: {e}"), true);
            }
        }
        #[cfg(not(windows))]
        let _ = (&webview, &handle, low, retrigger, background);
    });
    if let Err(e) = result {
        app.state::<Governor>().state.lock().unwrap_or_else(|p| p.into_inner()).low = None;
        status(app, format!("メモリ使用量の指定を WebView2 に伝えられませんでした: {e}"), true);
    }
}

/// Checks every few seconds whether work done while idle should be trimmed.
pub fn start_watch(app: &AppHandle) {
    let handle = app.clone();
    let spawned = std::thread::Builder::new().name("memory".into()).spawn(move || {
        loop {
            std::thread::sleep(SETTLE);
            handle.state::<Governor>().settled.store(true, Ordering::SeqCst);
            if let Some(window) = handle.get_webview_window("main") {
                update(&handle, &window);
            }
        }
    });
    if let Err(e) = spawned {
        status(app, format!("待機時のメモリ削減を開始できませんでした: {e}"), true);
    }
}

#[cfg(windows)]
mod windows_impl {
    use tauri::webview::PlatformWebview;
    use webview2_com::Microsoft::Web::WebView2::Win32::{
        COREWEBVIEW2_MEMORY_USAGE_TARGET_LEVEL_LOW, COREWEBVIEW2_MEMORY_USAGE_TARGET_LEVEL_NORMAL, ICoreWebView2_19,
    };
    use windows::Win32::Foundation::CloseHandle;
    use windows::Win32::System::Diagnostics::ToolHelp::{
        CreateToolhelp32Snapshot, PROCESSENTRY32W, Process32FirstW, Process32NextW, TH32CS_SNAPPROCESS,
    };
    use windows::Win32::System::ProcessStatus::K32EmptyWorkingSet;
    use windows::Win32::System::Threading::{
        GetCurrentProcess, GetCurrentProcessId, OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION, PROCESS_SET_QUOTA,
    };
    use windows_core::Interface;

    pub fn set_level(webview: &PlatformWebview, low: bool) -> windows_core::Result<()> {
        let level = if low {
            COREWEBVIEW2_MEMORY_USAGE_TARGET_LEVEL_LOW
        } else {
            COREWEBVIEW2_MEMORY_USAGE_TARGET_LEVEL_NORMAL
        };
        // SAFETY: the controller belongs to this window's web view and is used on its thread.
        unsafe {
            let core = webview.controller().CoreWebView2()?;
            core.cast::<ICoreWebView2_19>()?.SetMemoryUsageTargetLevel(level)
        }
    }

    /// Moves pages out of the working sets of this process and, with
    /// `descendants`, of the processes it started (directly or not); Windows
    /// brings them back on use. A failure only means less is given back, so
    /// it is not reported.
    pub fn trim_working_sets(descendants: bool) {
        // SAFETY: GetCurrentProcess returns a pseudo handle valid in this process.
        unsafe {
            let _ = K32EmptyWorkingSet(GetCurrentProcess());
        }
        if !descendants {
            return;
        }
        for pid in descendant_ids() {
            // SAFETY: the handle is checked, used for one call and closed.
            unsafe {
                if let Ok(handle) = OpenProcess(PROCESS_SET_QUOTA | PROCESS_QUERY_LIMITED_INFORMATION, false, pid) {
                    let _ = K32EmptyWorkingSet(handle);
                    let _ = CloseHandle(handle);
                }
            }
        }
    }

    /// Ids of the WebView2 processes descended from this one.
    fn descendant_ids() -> Vec<u32> {
        let mut pairs: Vec<(u32, u32, bool)> = Vec::new();
        // SAFETY: the snapshot handle is checked and closed; the entry struct
        // carries its size as the API requires.
        unsafe {
            let Ok(snapshot) = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0) else {
                return Vec::new();
            };
            let mut entry = PROCESSENTRY32W {
                dwSize: std::mem::size_of::<PROCESSENTRY32W>() as u32,
                ..Default::default()
            };
            if Process32FirstW(snapshot, &mut entry).is_ok() {
                loop {
                    let len = entry.szExeFile.iter().position(|&c| c == 0).unwrap_or(entry.szExeFile.len());
                    let name = String::from_utf16_lossy(&entry.szExeFile[..len]);
                    let webview = name.eq_ignore_ascii_case("msedgewebview2.exe");
                    pairs.push((entry.th32ProcessID, entry.th32ParentProcessID, webview));
                    if Process32NextW(snapshot, &mut entry).is_err() {
                        break;
                    }
                }
            }
            let _ = CloseHandle(snapshot);
        }
        // SAFETY: plain query of this process's id.
        let me = unsafe { GetCurrentProcessId() };
        let mut found = vec![me];
        let mut i = 0;
        while i < found.len() {
            let parent = found[i];
            for &(pid, ppid, webview) in &pairs {
                if webview && ppid == parent && pid != parent && !found.contains(&pid) {
                    found.push(pid);
                }
            }
            i += 1;
        }
        found.remove(0);
        found
    }
}
