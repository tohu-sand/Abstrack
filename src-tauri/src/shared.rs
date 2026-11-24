//! Page images handed to the window without a copy.
//!
//! WebView2 maps a buffer into this process and into the page's: the page is
//! drawn straight into it, and the page's script reads the same memory as an
//! ArrayBuffer. Through the IPC channel the same pixels cost a compression
//! here, a transfer, and a decompression on the page's thread, which for a
//! page filling a large screen is more than drawing it.
//!
//! WebView2's objects belong to the window's thread. The buffers are made,
//! posted and closed there and stay in a table of that thread; other threads
//! hold the address of the memory only.

use tauri::AppHandle;

/// A bitmap in memory shared with the page's script; closed when dropped.
/// The script keeps its own view until it releases it.
pub struct SharedImage {
    app: AppHandle,
    id: u64,
    start: *mut u8,
    len: usize,
}

// SAFETY: the memory stays mapped until the buffer is closed, which only
// dropping this value asks for, and this value is the only way to it here.
unsafe impl Send for SharedImage {}

impl SharedImage {
    /// `len` bytes, zeroed. `None` where there are no shared buffers (a web
    /// view other than WebView2, or one too old) or the window is gone.
    pub fn new(app: &AppHandle, len: usize) -> Option<SharedImage> {
        let (id, start) = imp::create(app, len)?;
        Some(SharedImage { app: app.clone(), id, start, len })
    }

    pub fn bytes(&self) -> &[u8] {
        // SAFETY: `start` points to `len` mapped bytes for as long as `self` lives.
        unsafe { std::slice::from_raw_parts(self.start, self.len) }
    }

    pub fn bytes_mut(&mut self) -> &mut [u8] {
        // SAFETY: as in `bytes`; the script's view is read-only.
        unsafe { std::slice::from_raw_parts_mut(self.start, self.len) }
    }

    /// Hands the image to the page's script, which is told `token` with it.
    pub fn post(&self, token: u32) -> Result<(), String> {
        imp::post(&self.app, self.id, token).map_err(|e| format!("ページ画像をウィンドウに渡せませんでした: {e}"))
    }
}

impl Drop for SharedImage {
    fn drop(&mut self) {
        imp::close(&self.app, self.id);
    }
}

#[cfg(windows)]
mod imp {
    use std::cell::RefCell;
    use std::collections::HashMap;
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::sync::mpsc::channel;
    use tauri::webview::PlatformWebview;
    use tauri::{AppHandle, Manager};
    use webview2_com::Microsoft::Web::WebView2::Win32::{
        COREWEBVIEW2_SHARED_BUFFER_ACCESS_READ_ONLY, ICoreWebView2_2, ICoreWebView2_17, ICoreWebView2Environment12,
        ICoreWebView2SharedBuffer,
    };
    use windows_core::{HSTRING, Interface};

    thread_local! {
        /// The buffers not closed yet, on the window's thread.
        static OPEN: RefCell<HashMap<u64, ICoreWebView2SharedBuffer>> = RefCell::default();
    }

    static NEXT: AtomicU64 = AtomicU64::new(0);

    /// Runs `f` on the window's thread and waits for what it returns.
    fn on_window_thread<T: Send + 'static>(
        app: &AppHandle,
        f: impl FnOnce(&PlatformWebview) -> windows_core::Result<T> + Send + 'static,
    ) -> Result<T, String> {
        let window = app.get_webview_window("main").ok_or("ウィンドウが閉じています")?;
        let (answer, waiting) = channel();
        window
            .with_webview(move |webview| {
                let _ = answer.send(f(&webview).map_err(|e| e.message()));
            })
            .map_err(|e| e.to_string())?;
        waiting.recv().map_err(|e| e.to_string())?
    }

    pub fn create(app: &AppHandle, len: usize) -> Option<(u64, *mut u8)> {
        let id = NEXT.fetch_add(1, Ordering::Relaxed);
        let start = on_window_thread(app, move |webview| {
            // SAFETY: the web view's objects are used on its own thread; the
            // address is that of the buffer kept in the table.
            unsafe {
                let core = webview.controller().CoreWebView2()?;
                let environment = core.cast::<ICoreWebView2_2>()?.Environment()?.cast::<ICoreWebView2Environment12>()?;
                let buffer = environment.CreateSharedBuffer(len as u64)?;
                let mut start = std::ptr::null_mut();
                buffer.Buffer(&mut start)?;
                OPEN.with_borrow_mut(|open| open.insert(id, buffer));
                Ok(start as usize)
            }
        })
        .ok()?;
        Some((id, start as *mut u8))
    }

    pub fn post(app: &AppHandle, id: u64, token: u32) -> Result<(), String> {
        on_window_thread(app, move |webview| {
            let message = HSTRING::from(format!("{{\"image\":{token}}}"));
            // SAFETY: as in `create`.
            unsafe {
                let core = webview.controller().CoreWebView2()?.cast::<ICoreWebView2_17>()?;
                OPEN.with_borrow(|open| match open.get(&id) {
                    Some(buffer) => core.PostSharedBufferToScript(buffer, COREWEBVIEW2_SHARED_BUFFER_ACCESS_READ_ONLY, &message),
                    None => Ok(()),
                })
            }
        })
    }

    /// Not waited for. When the window's thread has ended, the web view and
    /// its buffers have gone with it.
    pub fn close(app: &AppHandle, id: u64) {
        let _ = app.run_on_main_thread(move || {
            if let Some(buffer) = OPEN.with_borrow_mut(|open| open.remove(&id)) {
                // SAFETY: on the thread the buffer was made on.
                let _ = unsafe { buffer.Close() };
            }
        });
    }
}

#[cfg(not(windows))]
mod imp {
    use tauri::AppHandle;

    pub fn create(_: &AppHandle, _: usize) -> Option<(u64, *mut u8)> {
        None
    }

    pub fn post(_: &AppHandle, _: u64, _: u32) -> Result<(), String> {
        Ok(())
    }

    pub fn close(_: &AppHandle, _: u64) {}
}
