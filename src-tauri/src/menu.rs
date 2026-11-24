//! The window's menu bar, and the menus a right click shows. Every item does
//! something; items arrive with the features behind them. Items that act on
//! the view are passed to the window.

use tauri::menu::{AboutMetadata, CheckMenuItem, Menu, MenuItem, PredefinedMenuItem, Submenu};
use tauri::{AppHandle, Runtime};

pub const OPEN: &str = "file.open";
pub const SAVE: &str = "file.save";
pub const SAVE_AS: &str = "file.saveAs";
pub const CLOSE: &str = "file.close";
pub const QUIT: &str = "file.quit";
pub const RECENT_PREFIX: &str = "file.recent.";
pub const UNDO: &str = "edit.undo";
pub const REDO: &str = "edit.redo";

/// Menu ids handled by the window rather than by Rust. Saving and undoing go
/// through the window so that a comment being typed is finished first, and
/// Ctrl+Z in a text field undoes the typing.
pub const WINDOW_COMMANDS: &[&str] = &[
    SAVE,
    SAVE_AS,
    UNDO,
    REDO,
    "comment.highlight",
    "comment.replace",
    "comment.strikeOut",
    "comment.underline",
    "comment.insert",
    "comment.note",
    "comment.area",
    "comment.edit",
    "comment.reply",
    "comment.delete",
    "comment.check",
    "comment.copyEditText",
    "comment.copyUnchecked",
    "edit.copy",
    "edit.find",
    "edit.findNext",
    "edit.findPrevious",
    "edit.author",
    "edit.colors",
    "view.list",
    "view.fitWidth",
    "view.fitPage",
    "view.actualSize",
    "view.zoomIn",
    "view.zoomOut",
    "view.listLarger",
    "view.listSmaller",
    "view.searchList",
    "view.filterList",
    "go.nextPage",
    "go.previousPage",
    "go.firstPage",
    "go.lastPage",
    "go.page",
    "go.nextUnchecked",
    "go.previousUnchecked",
    "go.outline",
    "help.shortcuts",
];

#[derive(Clone, PartialEq)]
pub struct MenuState {
    pub recent: Vec<String>,
    pub list_visible: bool,
    pub has_document: bool,
    /// A document is open and not for viewing only.
    pub editable: bool,
    pub dirty: bool,
    /// What undo and redo would act on.
    pub undo: Option<String>,
    pub redo: Option<String>,
}

impl MenuState {
    /// Whether `other` has the same items, enabled the same way, apart from
    /// the few that change with every edit (see [`Live`]).
    pub fn same_layout(&self, other: &MenuState) -> bool {
        self.recent == other.recent && self.has_document == other.has_document && self.editable == other.editable
    }

    fn undo_label(&self) -> String {
        match &self.undo {
            Some(what) => format!("元に戻す(&U): {what}"),
            None => "元に戻す(&U)".to_string(),
        }
    }

    fn redo_label(&self) -> String {
        match &self.redo {
            Some(what) => format!("やり直し(&R): {what}"),
            None => "やり直し(&R)".to_string(),
        }
    }
}

/// The items that change with every edit, changed in place: building the
/// whole menu again takes the window's thread a long while (one round trip
/// per item).
pub struct Live<R: Runtime> {
    save: MenuItem<R>,
    undo: MenuItem<R>,
    redo: MenuItem<R>,
    list: CheckMenuItem<R>,
}

impl<R: Runtime> Live<R> {
    /// Changes the items that differ between `old` and `new`, which have the same layout.
    pub fn update(&self, old: &MenuState, new: &MenuState) -> tauri::Result<()> {
        if old.dirty != new.dirty {
            self.save.set_enabled(new.editable && new.dirty)?;
        }
        if old.undo != new.undo {
            self.undo.set_text(new.undo_label())?;
            self.undo.set_enabled(new.editable && new.undo.is_some())?;
        }
        if old.redo != new.redo {
            self.redo.set_text(new.redo_label())?;
            self.redo.set_enabled(new.editable && new.redo.is_some())?;
        }
        if old.list_visible != new.list_visible {
            self.list.set_checked(new.list_visible)?;
        }
        Ok(())
    }
}

pub fn build<R: Runtime>(app: &AppHandle<R>, state: &MenuState) -> tauri::Result<(Menu<R>, Live<R>)> {
    let item = |id: &str, label: &str, enabled: bool, accel: Option<&str>| MenuItem::with_id(app, id, label, enabled, accel);
    let doc = state.has_document;
    let editable = state.editable;

    let recent = Submenu::with_id(app, "file.recent", "最近使ったファイル(&R)", !state.recent.is_empty())?;
    for (i, path) in state.recent.iter().enumerate() {
        let label = format!("&{} {}", (i + 1) % 10, path.replace('&', "&&"));
        recent.append(&MenuItem::with_id(app, format!("{RECENT_PREFIX}{i}"), label, true, None::<&str>)?)?;
    }

    let save = item(SAVE, "上書き保存(&S)", editable && state.dirty, Some("CmdOrCtrl+S"))?;
    let file = Submenu::with_items(
        app,
        "ファイル(&F)",
        true,
        &[
            &item(OPEN, "開く(&O)...", true, Some("CmdOrCtrl+O"))?,
            &recent,
            &PredefinedMenuItem::separator(app)?,
            &save,
            &item(SAVE_AS, "名前を付けて保存(&A)...", editable, Some("CmdOrCtrl+Shift+S"))?,
            &PredefinedMenuItem::separator(app)?,
            &item(CLOSE, "閉じる(&C)", doc, Some("CmdOrCtrl+W"))?,
            &PredefinedMenuItem::separator(app)?,
            &item(QUIT, "終了(&X)", true, None)?,
        ],
    )?;
    let undo = item(UNDO, &state.undo_label(), editable && state.undo.is_some(), Some("CmdOrCtrl+Z"))?;
    let redo = item(REDO, &state.redo_label(), editable && state.redo.is_some(), Some("CmdOrCtrl+Shift+Z"))?;
    // Single keys are shown after a tab but not registered as accelerators:
    // the menu would take them even from a text field.
    let comment = Submenu::with_items(
        app,
        "コメントを入れる(&M)",
        editable,
        &[
            &item("comment.highlight", "ハイライト(&H)\tH", editable, None)?,
            &item("comment.replace", "置換(&R)\tR", editable, None)?,
            &item("comment.strikeOut", "取り消し線(&D)\tD", editable, None)?,
            &item("comment.underline", "下線(&U)\tU", editable, None)?,
            &item("comment.insert", "挿入(&I)\tI", editable, None)?,
            &item("comment.note", "メモ(&N)\tN", editable, None)?,
            &item("comment.area", "長方形(&A)\tEnter", editable, None)?,
        ],
    )?;
    let edit = Submenu::with_items(
        app,
        "編集(&E)",
        true,
        &[
            &undo,
            &redo,
            &PredefinedMenuItem::separator(app)?,
            &item("edit.copy", "コピー(&C)", true, Some("CmdOrCtrl+C"))?,
            &PredefinedMenuItem::separator(app)?,
            &comment,
            &item("comment.edit", "コメントを編集(&E)\tEnter", editable, None)?,
            &item("comment.reply", "返信(&Y)", editable, None)?,
            &item("comment.delete", "コメントを削除(&L)\tDelete", editable, None)?,
            &item("comment.check", "チェックを付ける・外す(&K)\tSpace", editable, None)?,
            &PredefinedMenuItem::separator(app)?,
            &item("comment.copyEditText", "置換後の文字列をコピー(&T)", doc, None)?,
            &item("comment.copyUnchecked", "未対応の一覧をコピー(&O)", doc, None)?,
            &PredefinedMenuItem::separator(app)?,
            &item("edit.find", "検索(&F)...", doc, Some("CmdOrCtrl+F"))?,
            &item("edit.findNext", "次を検索(&N)", doc, Some("F3"))?,
            &item("edit.findPrevious", "前を検索(&P)", doc, Some("Shift+F3"))?,
            &PredefinedMenuItem::separator(app)?,
            &item("edit.author", "コメントの作成者名(&A)...", true, None)?,
            &item("edit.colors", "コメントの色(&I)...", true, None)?,
        ],
    )?;
    let list = CheckMenuItem::with_id(app, "view.list", "コメント一覧(&L)", true, state.list_visible, Some("F4"))?;
    let view = Submenu::with_items(
        app,
        "表示(&V)",
        true,
        &[
            &list,
            &PredefinedMenuItem::separator(app)?,
            &item("view.fitWidth", "幅に合わせる(&W)", doc, Some("CmdOrCtrl+2"))?,
            &item("view.fitPage", "ページ全体(&P)", doc, Some("CmdOrCtrl+0"))?,
            &item("view.actualSize", "実際のサイズ(&A)", doc, Some("CmdOrCtrl+1"))?,
            &item("view.zoomIn", "拡大(&I)", doc, Some("CmdOrCtrl+Shift+Equal"))?,
            &item("view.zoomOut", "縮小(&O)", doc, Some("CmdOrCtrl+Shift+Minus"))?,
            &PredefinedMenuItem::separator(app)?,
            &item("view.listLarger", "一覧の文字を大きく(&G)", true, Some("CmdOrCtrl+Equal"))?,
            &item("view.listSmaller", "一覧の文字を小さく(&S)", true, Some("CmdOrCtrl+Minus"))?,
            &PredefinedMenuItem::separator(app)?,
            &item("view.searchList", "コメントを検索(&F)...", doc, Some("CmdOrCtrl+Shift+F"))?,
            &item("view.filterList", "コメントを絞り込む(&R)...", doc, None)?,
        ],
    )?;
    let go = Submenu::with_items(
        app,
        "移動(&G)",
        true,
        &[
            &item("go.nextPage", "次のページ(&N)", doc, Some("CmdOrCtrl+PageDown"))?,
            &item("go.previousPage", "前のページ(&P)", doc, Some("CmdOrCtrl+PageUp"))?,
            &item("go.firstPage", "最初のページ(&F)", doc, Some("CmdOrCtrl+Home"))?,
            &item("go.lastPage", "最後のページ(&L)", doc, Some("CmdOrCtrl+End"))?,
            &item("go.page", "ページを指定(&G)...", doc, Some("CmdOrCtrl+G"))?,
            &PredefinedMenuItem::separator(app)?,
            &item("go.nextUnchecked", "次の未対応へ(&U)", doc, Some("F8"))?,
            &item("go.previousUnchecked", "前の未対応へ(&D)", doc, Some("Shift+F8"))?,
            &PredefinedMenuItem::separator(app)?,
            &item("go.outline", "しおり(&B)...", doc, Some("CmdOrCtrl+B"))?,
        ],
    )?;
    let about = AboutMetadata {
        name: Some("Abstrack".into()),
        version: Some(app.package_info().version.to_string()),
        ..Default::default()
    };
    let help = Submenu::with_items(
        app,
        "ヘルプ(&H)",
        true,
        &[
            &item("help.shortcuts", "ショートカット一覧(&K)", true, Some("F1"))?,
            &PredefinedMenuItem::separator(app)?,
            &PredefinedMenuItem::about(app, Some("バージョン情報(&A)"), Some(about))?,
        ],
    )?;
    let menu = Menu::with_items(app, &[&file, &edit, &view, &go, &help])?;
    Ok((menu, Live { save, undo, redo, list }))
}

/// The menu of a right click: `(id, text)` pairs naming commands of the menu
/// bar, so that a choice is passed to the window as the menu bar's is. An
/// empty id draws a line.
pub fn context<R: Runtime>(app: &AppHandle<R>, entries: &[(String, String)]) -> tauri::Result<Menu<R>> {
    let menu = Menu::new(app)?;
    for (id, text) in entries {
        if id.is_empty() {
            menu.append(&PredefinedMenuItem::separator(app)?)?;
        } else {
            menu.append(&MenuItem::with_id(app, id.as_str(), text, true, None::<&str>)?)?;
        }
    }
    Ok(menu)
}
