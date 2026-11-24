#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod annotate;
mod ask;
mod bitmaps;
mod checks;
mod clock;
mod commands;
mod comments;
mod document;
mod edit;
mod engine;
mod memory;
mod marked;
mod menu;
mod open;
mod placement;
mod recovery;
mod restore;
mod save;
mod selection;
mod settings;
mod shared;
mod state;
mod texts;
mod unsaved;
mod watch;

use engine::EngineHandle;
use settings::Settings;
use state::{AppState, lock, refresh_menu, save_settings, status};
use std::path::PathBuf;
use std::sync::Mutex;
use std::sync::atomic::AtomicU64;
use tauri::{AppHandle, DragDropEvent, Emitter, Manager, RunEvent, WindowEvent};

fn open_with_dialog(app: &AppHandle) {
    let handle = app.clone();
    ask::files(app)
        .add_filter("PDF", &["pdf"])
        .pick_file(move |picked| {
            if let Some(picked) = picked {
                match picked.into_path() {
                    Ok(path) => open::open_path(&handle, &path),
                    Err(e) => status(&handle, format!("選択したファイルの場所を扱えません: {e}"), true),
                }
            }
        });
}

fn first_pdf(paths: &[PathBuf]) -> Option<&PathBuf> {
    paths
        .iter()
        .find(|p| p.extension().is_some_and(|e| e.eq_ignore_ascii_case("pdf")))
}

pub fn save_as_with_dialog(app: &AppHandle) {
    let doc = lock(&app.state::<AppState>().document).clone();
    let Some(doc) = doc else { return };
    let path = doc.path();
    let mut dialog = ask::files(app).add_filter("PDF", &["pdf"]).set_file_name(doc.file_name());
    if let Some(dir) = path.parent() {
        dialog = dialog.set_directory(dir);
    }
    let handle = app.clone();
    dialog.save_file(move |picked| {
        let Some(picked) = picked else { return };
        match picked.into_path() {
            Ok(target) => {
                let worker = handle.clone();
                std::thread::spawn(move || {
                    if let Err(e) = save::save(&worker, Some(target)) {
                        status(&worker, e, true);
                    }
                });
            }
            Err(e) => status(&handle, format!("保存先の場所を扱えません: {e}"), true),
        }
    });
}

fn on_menu(app: &AppHandle, id: &str) {
    match id {
        menu::OPEN => unsaved::confirm(app, open_with_dialog),
        menu::CLOSE => unsaved::confirm(app, open::close_document),
        menu::QUIT => unsaved::confirm(app, |app| app.exit(0)),
        _ if id.starts_with(menu::RECENT_PREFIX) => {
            let index: Option<usize> = id[menu::RECENT_PREFIX.len()..].parse().ok();
            let path = index.and_then(|i| lock(&app.state::<AppState>().settings).recent_files.get(i).cloned());
            if let Some(path) = path {
                unsaved::confirm(app, move |app| open::open_path(app, &PathBuf::from(path)));
            }
        }
        _ if menu::WINDOW_COMMANDS.contains(&id) => {
            if let Err(e) = app.emit("menu", id) {
                eprintln!("menu event failed: {e}");
            }
        }
        _ => {}
    }
}

fn main() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .setup(|app| {
            let settings_path = app.path().app_config_dir().ok().map(|d| d.join("settings.json"));
            let (settings, problem) = match &settings_path {
                Some(path) => Settings::load(path),
                None => (Settings::default(), Some("設定の保存場所が分かりません。設定は保存されません。".into())),
            };
            app.manage(memory::Governor::default());
            app.manage(AppState {
                document: Mutex::new(None),
                comments: Mutex::new(None),
                places: Mutex::new(Default::default()),
                generation: AtomicU64::new(0),
                engine: EngineHandle::start(engine::library_dir()),
                settings: Mutex::new(settings),
                menu: Mutex::new(None),
                settings_path,
                last_status: Mutex::new(None),
            });
            let handle = app.handle();
            let recovery_dir = app.path().app_local_data_dir().ok().map(|d| d.join("recovery"));
            if recovery_dir.is_none() {
                status(handle, "復元用データの保存場所が分かりません。異常終了すると、保存していない変更は戻せません。", true);
            }
            app.manage(recovery::Recovery::start(handle, recovery_dir));
            watch::start(handle);
            // The web view takes a few hundred milliseconds to come up, with
            // the window's thread waiting for it: the document is read and
            // its first page drawn meanwhile.
            let args: Vec<PathBuf> = std::env::args_os().skip(1).map(PathBuf::from).collect();
            match first_pdf(&args) {
                Some(path) => {
                    let path = std::path::absolute(path).unwrap_or_else(|_| path.clone());
                    state::expect_document_menu(handle, &path);
                    open::open_path(handle, &path);
                }
                None => refresh_menu(handle),
            }
            let saved = lock(&handle.state::<AppState>().settings).window;
            let (window, placed) = placement::open_window(handle, saved)?;
            if let Some(placed) = placed
                && let Err(e) = placement::settle(&window, placed)
            {
                status(handle, format!("前回のウィンドウの位置に戻せませんでした: {e}"), true);
            }
            // A document opened before the window was there could not name it.
            let open = lock(&handle.state::<AppState>().document).clone();
            if let Some(doc) = open {
                state::set_title(handle, &state::window_title(&doc.file_name(), doc.edit().is_dirty()));
            }
            memory::start_watch(handle);
            if let Some(problem) = problem {
                status(handle, problem, true);
            }
            Ok(())
        })
        .on_menu_event(|app, event| on_menu(app, event.id().as_ref()))
        .on_window_event(|window, event| match event {
            WindowEvent::DragDrop(DragDropEvent::Drop { paths, .. }) => {
                let app = window.app_handle();
                match first_pdf(paths).cloned() {
                    Some(path) => unsaved::confirm(app, move |app| open::open_path(app, &path)),
                    None => status(app, "PDFファイルをドロップしてください", true),
                }
            }
            WindowEvent::Focused(_) | WindowEvent::Resized(_) => {
                let app = window.app_handle();
                if let Some(webview) = app.get_webview_window(window.label()) {
                    memory::update(app, &webview);
                }
            }
            WindowEvent::CloseRequested { api, .. } => {
                let app = window.app_handle();
                if let Some(webview) = app.get_webview_window(window.label()) {
                    let state = app.state::<AppState>();
                    let previous = lock(&state.settings).window;
                    let current = placement::capture(&webview, previous);
                    lock(&state.settings).window = current;
                    save_settings(app);
                }
                let dirty = lock(&app.state::<AppState>().document).as_ref().is_some_and(|d| d.edit().is_dirty());
                if dirty {
                    api.prevent_close();
                    unsaved::confirm(app, |app| app.exit(0));
                }
            }
            _ => {}
        })
        .invoke_handler(tauri::generate_handler![
            commands::document_info,
            commands::comments,
            commands::outline,
            commands::current_status,
            commands::set_visible_pages,
            commands::render_page,
            commands::render_region,
            commands::recent_files,
            commands::open_file,
            commands::ui_settings,
            commands::set_ui_settings,
            commands::search,
            commands::set_page_idle,
            commands::prefetch_pages,
            commands::caret_at,
            commands::select_range,
            commands::unit_at,
            commands::move_caret,
            commands::create_comment,
            commands::set_comment_text,
            commands::delete_comment,
            commands::reply_to_comment,
            commands::set_comment_marked,
            commands::save_document,
            commands::save_document_as,
            commands::undo_change,
            commands::reload_document,
            commands::recovery_offers,
            commands::restore_recovery,
            commands::discard_recovery,
            commands::comment_range,
            commands::reshape_comment,
            commands::author_name,
            commands::set_author_name,
            commands::comment_colors,
            commands::set_comment_colors,
            commands::show_context_menu,
        ])
        .build(tauri::generate_context!())
        .unwrap_or_else(|e| {
            eprintln!("Abstrack failed to start: {e}");
            std::process::exit(1);
        })
        .run(|app, event| {
            // Every way out asked before giving changes up, so the copy kept
            // against a crash is no longer needed.
            if let RunEvent::Exit = event {
                app.state::<recovery::Recovery>().finish();
            }
        });
}
