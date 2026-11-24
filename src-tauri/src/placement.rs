//! Remembering where the window was.

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, PhysicalPosition, PhysicalSize, Runtime, WebviewWindow, WebviewWindowBuilder};

/// Where the window's outer corner is and how large it is inside its frame
/// and menu bar, in physical pixels.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Placement {
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
    pub maximized: bool,
}

/// The window's current placement. While maximized, the bounds it had before
/// (from `previous`) are kept, so that un-maximizing after a restart returns there.
pub fn capture<R: Runtime>(window: &WebviewWindow<R>, previous: Option<Placement>) -> Option<Placement> {
    let maximized = window.is_maximized().ok()?;
    if window.is_minimized().ok()? {
        return previous;
    }
    if maximized {
        return previous.map(|p| Placement { maximized: true, ..p });
    }
    let position = window.outer_position().ok()?;
    let size = window.inner_size().ok()?;
    Some(Placement {
        x: position.x,
        y: position.y,
        width: size.width,
        height: size.height,
        maximized: false,
    })
}

/// Makes the main window, where it was last unless its title bar would land
/// outside every monitor; the placement it was given, if any. The window is
/// made in its place and with the menu bar: moving it or giving it a menu
/// afterwards changes the size of the web view while it loads the page, and
/// holds the page up.
pub fn open_window(app: &AppHandle, saved: Option<Placement>) -> tauri::Result<(WebviewWindow, Option<Placement>)> {
    let config = app.config().app.windows.first().ok_or(tauri::Error::WindowNotFound)?;
    let mut builder = WebviewWindowBuilder::from_config(app, config)?;
    let placed = saved.and_then(|p| Some((p, scale_where(app, p)?)));
    if let Some((p, scale)) = placed {
        builder = builder
            .inner_size(p.width as f64 / scale, p.height as f64 / scale)
            .position(p.x as f64 / scale, p.y as f64 / scale)
            .maximized(p.maximized);
    }
    Ok((builder.build()?, placed.map(|(p, _)| p)))
}

/// The scale factor of the monitor that shows the top of a window placed at
/// `p`; `None` when none does, or the window would be too small to use.
fn scale_where(app: &AppHandle, p: Placement) -> Option<f64> {
    if p.width < 400 || p.height < 300 {
        return None;
    }
    let monitors = app.available_monitors().ok()?;
    monitors.iter().find_map(|m| {
        let pos = m.position();
        let size = m.size();
        let (mx0, my0) = (pos.x, pos.y);
        let (mx1, my1) = (pos.x + size.width as i32, pos.y + size.height as i32);
        // The top 40 pixels of the window, where the title bar is, must be on the monitor.
        let reachable = p.x + 100 < mx1 && p.x + p.width as i32 - 100 > mx0 && p.y >= my0 && p.y + 40 < my1;
        reachable.then(|| m.scale_factor())
    })
}

/// Corrects a window that did not come up exactly at `p`, as can happen
/// between monitors of different scale factors.
pub fn settle<R: Runtime>(window: &WebviewWindow<R>, p: Placement) -> tauri::Result<()> {
    if p.maximized {
        return Ok(());
    }
    if window.inner_size()? != PhysicalSize::new(p.width, p.height) {
        window.set_size(PhysicalSize::new(p.width, p.height))?;
    }
    if window.outer_position()? != PhysicalPosition::new(p.x, p.y) {
        window.set_position(PhysicalPosition::new(p.x, p.y))?;
    }
    Ok(())
}
