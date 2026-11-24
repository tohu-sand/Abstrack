// What the menu items the window handles do; their ids are listed in
// src-tauri/src/menu.rs. Each acts through the parts of the window.

import type { CommentRow } from "./backend";
import type { Checking } from "./checking";
import type { Commenting } from "./commenting.svelte";
import type { EditCommands } from "./editCommands";
import { nextZoom } from "./layout";
import type { ListPanel } from "./listPanel.svelte";
import type { Searching } from "./searching.svelte";
import type { CommentKind } from "./selection";
import type { Zooming } from "./zooming.svelte";

export interface MenuParts {
  edits: EditCommands;
  search: Searching;
  commenting: Commenting;
  checking: Checking;
  list: ListPanel;
  view: Zooming;
  selectedKey: () => string | null;
  selectedRow: () => CommentRow | undefined;
  /** Shows a panel over the window. */
  open: (panel: "outline" | "help" | "author" | "colors") => void;
  hasOutline: () => boolean;
  currentPage: () => number;
  pageCount: () => number;
  goToPage: (page: number) => void;
  focusPageField: () => void;
  /** The search field and the filters of the comment list, showing the list first. */
  searchList: () => void;
  filterList: () => void;
  message: (text: string) => void;
}

export function menuCommands(p: MenuParts): Record<string, () => void> {
  const comment = (kind: CommentKind) => () => p.commenting.create(kind);
  return {
    "file.save": p.edits.save,
    "file.saveAs": p.edits.saveAs,
    "edit.undo": () => p.edits.undoOrRedo(false),
    "edit.redo": () => p.edits.undoOrRedo(true),
    "edit.copy": p.edits.copy,
    "edit.author": () => p.open("author"),
    "edit.colors": () => p.open("colors"),
    "edit.find": () => p.search.show(),
    "edit.findNext": () => p.search.step(1),
    "edit.findPrevious": () => p.search.step(-1),
    "comment.highlight": comment("highlight"),
    "comment.replace": comment("replace"),
    "comment.strikeOut": comment("strikeOut"),
    "comment.underline": comment("underline"),
    "comment.insert": comment("insert"),
    "comment.note": comment("note"),
    "comment.area": comment("area"),
    "comment.edit": () => p.commenting.edit(p.selectedKey()),
    "comment.reply": () => p.commenting.reply(p.selectedKey()),
    "comment.delete": () => p.commenting.remove(p.selectedKey()),
    "comment.check": () => p.checking.toggle(p.selectedKey()),
    "comment.copyEditText": () => {
      const row = p.selectedRow();
      if (row && row.editText !== null) void p.checking.copyEditText(row);
      else p.message("置換か挿入のコメントを選んでください");
    },
    "comment.copyUnchecked": () => p.checking.copyUnchecked(),
    "view.list": () => p.list.toggle(),
    "view.searchList": p.searchList,
    "view.filterList": p.filterList,
    "view.fitWidth": () => p.view.fitTo("width"),
    "view.fitPage": () => p.view.fitTo("page"),
    "view.actualSize": () => p.view.to(1),
    "view.zoomIn": () => p.view.to(nextZoom(p.view.zoom, 1)),
    "view.zoomOut": () => p.view.to(nextZoom(p.view.zoom, -1)),
    "view.listLarger": () => p.list.setFont(p.list.fontSize + 1),
    "view.listSmaller": () => p.list.setFont(p.list.fontSize - 1),
    "go.nextPage": () => p.goToPage(p.currentPage() + 1),
    "go.previousPage": () => p.goToPage(p.currentPage() - 1),
    "go.firstPage": () => p.goToPage(0),
    "go.lastPage": () => p.goToPage(p.pageCount() - 1),
    "go.page": p.focusPageField,
    "go.nextUnchecked": () => p.checking.next(1),
    "go.previousUnchecked": () => p.checking.next(-1),
    "go.outline": () => {
      if (p.hasOutline()) p.open("outline");
      else p.message("この文書にはしおりがありません");
    },
    "help.shortcuts": () => p.open("help"),
  };
}
