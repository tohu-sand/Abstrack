import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { collectSoon } from "./gc";
import type { CommentKind } from "./selection";
import { expectSharedImage, releaseSharedImage } from "./sharedImages";

export interface PageInfo {
  /** Displayed size in points (rotation and UserUnit applied). */
  width: number;
  height: number;
  /** Visible area in user space: [x0, y0, x1, y1]. */
  visible: [number, number, number, number];
  rotation: number;
  userUnit: number;
}

/** How the file on disk stands against the bytes the document was read from. */
export type Outside = "same" | "changed" | "missing";

export interface DocumentInfo {
  generation: number;
  path: string;
  fileName: string;
  readOnlyReason: string | null;
  commentCount: number;
  /** Changes not saved yet. */
  dirty: boolean;
  outside: Outside;
  /** Per page, bumped whenever its annotations change: the page is drawn again. */
  pageRevisions: number[];
  pages: PageInfo[];
}

export interface ReplyRow {
  author: string;
  date: string;
  text: string;
}

/** Rows come in reading order: a row's number (#1, #2, ...) is its place there. */
export interface CommentRow {
  key: string;
  /** Changes whenever anything shown in the row changes. */
  version: string;
  page: number;
  kind: string;
  color: string | null;
  target: string;
  editText: string | null;
  body: string | null;
  author: string;
  date: string;
  timestamp: number;
  replies: ReplyRow[];
  marked: boolean;
  review: string | null;
  /** Bounds of the annotation, user space [x0, y0, x1, y1]. */
  rect: [number, number, number, number];
  /** QuadPoints entries, user space, 8 numbers each. */
  quads: number[][];
}

export interface SearchHit {
  page: number;
  /** One rectangle per line, user space [x0, y0, x1, y1]. */
  rects: number[][];
  /** Carets before the first and after the last character found, in the page's reading order. */
  start: number;
  end: number;
}

export interface Caret {
  page: number;
  index: number;
}

export interface SelectionInfo {
  pages: { page: number; rects: number[][] }[];
  text: string;
}

export type CaretUnit = "char" | "word" | "line" | "sentence";

export interface Range {
  anchor: Caret;
  focus: Caret;
}

/** A comment to make from what is selected on the page. */
export type CommentRequest =
  | { kind: "highlight" | "strikeOut" | "underline" | "replace"; ranges: Range[] }
  | { kind: "insert"; at: Caret; after: boolean }
  | { kind: "note"; page: number; x: number; y: number }
  | { kind: "area"; page: number; rect: [number, number, number, number] };

/** Sent after every change to the comments. */
export interface Changed {
  generation: number;
  dirty: boolean;
  pageRevisions: number[];
  /** Where pages look different now, user space: only there needs drawing again. */
  damage: { page: number; rect: [number, number, number, number] }[];
}

export interface OutlineItem {
  level: number;
  title: string;
  page: number;
}

export interface UiSettings {
  listVisible: boolean;
  listWidth: number;
  listFontSize: number;
}

/** Changes a run that ended abruptly did not save. */
export interface RecoveryOffer {
  id: string;
  path: string;
  fileName: string;
  /** When they were last kept, as shown. */
  written: string;
}

export interface StatusMessage {
  text: string;
  isError: boolean;
}

export const documentInfo = () => invoke<DocumentInfo | null>("document_info");
export const comments = () => invoke<CommentRow[] | null>("comments");
export const outline = () => invoke<OutlineItem[]>("outline");
export const currentStatus = () => invoke<StatusMessage | null>("current_status");
export const recentFiles = () => invoke<string[]>("recent_files");
export const openFile = (path: string) => invoke<void>("open_file", { path });
export const uiSettings = () => invoke<UiSettings>("ui_settings");
export const setUiSettings = (s: UiSettings) => invoke<void>("set_ui_settings", { ...s });
export const setVisiblePages = (pages: number[]) => invoke<void>("set_visible_pages", { pages });
export const search = (generation: number, query: string, start: number, end: number) =>
  invoke<SearchHit[]>("search", { generation, query, start, end });
export const searchCounts = (generation: number, query: string, start: number, end: number) =>
  invoke<number[]>("search", { generation, query, start, end, countsOnly: true });
export const prefetchPages = (generation: number, pages: { page: number; width: number; height: number }[]) =>
  invoke<void>("prefetch_pages", { generation, pages });
export const caretAt = (generation: number, page: number, x: number, y: number) =>
  invoke<Caret | null>("caret_at", { generation, page, x, y });
export const selectRange = (generation: number, anchor: Caret, focus: Caret) =>
  invoke<SelectionInfo>("select_range", { generation, anchor, focus });
export const unitAt = (generation: number, at: Caret, unit: CaretUnit) =>
  invoke<[Caret, Caret]>("unit_at", { generation, at, unit });
/** `after`: a caret at a line break belongs to the end of the earlier line. */
export const moveCaret = (generation: number, at: Caret, unit: CaretUnit, forward: boolean, after: boolean) =>
  invoke<Caret>("move_caret", { generation, at, unit, forward, after });

/** Makes a comment; the key of its row in the comment list. */
export const createComment = (generation: number, request: CommentRequest, contents: string) =>
  invoke<string>("create_comment", { generation, request, contents });
export const setCommentText = (generation: number, key: string, text: string) =>
  invoke<void>("set_comment_text", { generation, key, text });
export const deleteComment = (generation: number, key: string) => invoke<void>("delete_comment", { generation, key });
export const replyToComment = (generation: number, key: string, text: string) =>
  invoke<void>("reply_to_comment", { generation, key, text });
/** Checks a comment as dealt with (Acrobat's check mark), or takes the check off. */
export const setCommentMarked = (generation: number, key: string, marked: boolean) =>
  invoke<void>("set_comment_marked", { generation, key, marked });
export const saveDocument = () => invoke<void>("save_document");
export const saveDocumentAs = () => invoke<void>("save_document_as");
/** Undoes (or redoes) the last change to the comments; what it was. */
export const undoChange = (generation: number, redo: boolean) => invoke<string | null>("undo_change", { generation, redo });
/** The carets around what a comment marks, when the handles on the page can move them. */
export const commentRange = (generation: number, key: string) =>
  invoke<{ start: Caret; end: Caret } | null>("comment_range", { generation, key });
export const reshapeComment = (generation: number, key: string, range: Range) =>
  invoke<void>("reshape_comment", { generation, key, range });
export const reloadDocument = () => invoke<void>("reload_document");
export const recoveryOffers = () => invoke<RecoveryOffer[]>("recovery_offers");
export const restoreRecovery = (id: string) => invoke<void>("restore_recovery", { id });
export const discardRecovery = (id: string) => invoke<void>("discard_recovery", { id });

export interface PageImage {
  /** RGBA, `width` x `height` pixels. */
  rgba: ArrayBuffer;
  /** Gives the memory back once the image is on a canvas; `rgba` cannot be read afterwards. */
  release: () => void;
}

// Whether this web view can inflate deflate-raw (the constructor throws where it
// cannot); pages are asked for uncompressed otherwise.
const inflates = (() => {
  try {
    new DecompressionStream("deflate-raw");
    return true;
  } catch {
    return false;
  }
})();

/** A page drawn at `width` x `height` pixels; `null` when `cancelled` by the time it is there. */
export async function renderPage(generation: number, page: number, width: number, height: number, cancelled: () => boolean): Promise<PageImage | null> {
  const expected = expectSharedImage();
  let inline: ArrayBuffer | null;
  try {
    inline = await invoke<ArrayBuffer | null>("render_page", { generation, page, width, height, compressed: inflates, shared: expected?.token });
  } catch (e) {
    expected?.forget();
    throw e;
  }
  if (inline === null) {
    if (!expected) throw new Error("ページ画像が届きませんでした");
    const rgba = await expected.arrived;
    if (!cancelled()) return { rgba, release: () => releaseSharedImage(rgba) };
    releaseSharedImage(rgba);
    return null;
  }
  expected?.forget();
  if (cancelled()) return null;
  if (!inflates) return { rgba: inline, release: collectSoon };
  const reader = new Response(inline).body!.pipeThrough(new DecompressionStream("deflate-raw")).getReader();
  const rgba = new Uint8Array(width * height * 4);
  let offset = 0;
  for (;;) {
    const { value, done } = await reader.read();
    if (cancelled()) {
      await reader.cancel();
      return null;
    }
    if (done) break;
    rgba.set(value, offset);
    offset += value.length;
  }
  if (offset !== rgba.length) throw new Error("ページ画像の大きさが一致しません");
  return { rgba: rgba.buffer, release: collectSoon };
}

/** RGBA bytes of the part (x, y, w, h) of a page drawn at `width` x `height` pixels. */
export function renderRegion(
  generation: number,
  page: number,
  size: { width: number; height: number },
  region: { x: number; y: number; w: number; h: number },
): Promise<ArrayBuffer> {
  return invoke<ArrayBuffer>("render_region", { generation, page, ...size, ...region });
}

/** Errors that only mean the request no longer matters. */
export function isDiscarded(error: unknown): boolean {
  const text = String(error);
  return text === "skipped" || text === "stale";
}

export const onDocumentChanged = (f: () => void): Promise<UnlistenFn> => listen("document-changed", () => f());
export const onCommentsChanged = (f: () => void): Promise<UnlistenFn> => listen("comments-changed", () => f());
export const onAnnotationsChanged = (f: (c: Changed) => void): Promise<UnlistenFn> =>
  listen<Changed>("annotations-changed", (e) => f(e.payload));
export const onStatus = (f: (m: StatusMessage) => void): Promise<UnlistenFn> =>
  listen<StatusMessage>("status", (e) => f(e.payload));
export const onMenu = (f: (id: string) => void): Promise<UnlistenFn> => listen<string>("menu", (e) => f(e.payload));
export const onFileOutside = (f: (r: { generation: number; outside: Outside }) => void): Promise<UnlistenFn> =>
  listen<{ generation: number; outside: Outside }>("file-outside", (e) => f(e.payload));
export const onRecoveryChanged = (f: () => void): Promise<UnlistenFn> => listen("recovery-changed", () => f());
export const authorName = () => invoke<{ name: string; login: string }>("author_name");
export const setAuthorName = (name: string) => invoke<void>("set_author_name", { name });

/** The color new comments of a kind get. */
export interface KindColor {
  kind: CommentKind;
  /** `#rrggbb`. */
  color: string;
  /** Chosen in place of the default. */
  changed: boolean;
}
export const commentColors = () => invoke<KindColor[]>("comment_colors");
/** Chooses colors, `#rrggbb` by kind; the kinds left out take their defaults. */
export const setCommentColors = (colors: Partial<Record<CommentKind, string>>) =>
  invoke<void>("set_comment_colors", { colors });

/** An item of a right-click menu: the id of a menu bar command and the text shown; an empty id is a line. */
export interface MenuEntry {
  id: string;
  text: string;
}
/** Shows a menu at the pointer; a choice comes back through `onMenu` as the menu bar's does. */
export const showContextMenu = (entries: MenuEntry[]) => invoke<void>("show_context_menu", { entries });
