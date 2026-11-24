// Making and changing comments from the window: a key or button asks for a
// comment, the application makes it, and its row in the comment list opens
// a field for the text. Every change is made by the application, which can
// undo it.

import * as api from "./backend";
import type { CommentRequest, CommentRow, DocumentInfo } from "./backend";
import type { CommentKind } from "./selection";

export interface Editing {
  key: string;
  /** "body": the comment's own text; "reply": a new reply to it. */
  mode: "body" | "reply";
}

interface Host {
  doc: () => DocumentInfo | null;
  /** What a comment of `kind` would mark now, or what to do first. */
  request: (kind: CommentKind) => CommentRequest | string | undefined;
  clearSelection: () => void;
  /** Shows the comment list and selects the row. */
  reveal: (key: string) => void;
  message: (text: string) => void;
  error: (text: string) => void;
}

export class Commenting {
  editing = $state<Editing | null>(null);

  constructor(private host: Host) {}

  async create(kind: CommentKind): Promise<void> {
    const doc = this.host.doc();
    if (!doc) return;
    if (doc.readOnlyReason) {
      this.host.error(doc.readOnlyReason);
      return;
    }
    const request = this.host.request(kind);
    if (request === undefined) return;
    if (typeof request === "string") {
      this.host.message(request);
      return;
    }
    try {
      const key = await api.createComment(doc.generation, request, "");
      this.host.clearSelection();
      this.host.reveal(key);
      this.editing = { key, mode: "body" };
    } catch (e) {
      this.host.error(`コメントを入れられませんでした: ${String(e)}`);
    }
  }

  edit(key: string | null): void {
    this.open(key, "body");
  }

  reply(key: string | null): void {
    this.open(key, "reply");
  }

  /** The field is in the comment's row, so the list is shown first. */
  private open(key: string | null, mode: Editing["mode"]): void {
    if (!key || this.host.doc()?.readOnlyReason) return;
    this.host.reveal(key);
    this.editing = { key, mode };
  }

  /**
   * Keeps the text typed into the field opened for `field`: on Enter, or
   * when the field loses the focus, also because another one opens.
   */
  async commit(field: Editing, text: string): Promise<void> {
    this.close(field);
    const doc = this.host.doc();
    if (!doc) return;
    try {
      if (field.mode === "reply") {
        if (text.trim()) await api.replyToComment(doc.generation, field.key, text);
      } else {
        await api.setCommentText(doc.generation, field.key, text);
      }
    } catch (e) {
      this.host.error(`コメントを書き込めませんでした: ${String(e)}`);
    }
  }

  /** Esc: a new comment stays without text, an edit keeps the earlier text. */
  cancel(field: Editing): void {
    this.close(field);
  }

  private close(field: Editing): void {
    if (this.editing?.key === field.key && this.editing.mode === field.mode) this.editing = null;
  }

  async remove(key: string | null): Promise<void> {
    const doc = this.host.doc();
    if (!key || !doc || doc.readOnlyReason) return;
    try {
      await api.deleteComment(doc.generation, key);
      this.host.message("コメントを削除しました");
    } catch (e) {
      this.host.error(`コメントを削除できませんでした: ${String(e)}`);
    }
  }
}

/** The text a row's field starts with: for a replacement or an insertion,
 * the text to put in on the first line and the instructions after it. */
export function editableText(row: CommentRow): string {
  if (row.kind === "置換" || row.kind === "挿入") {
    return [row.editText ?? "", row.body ?? ""].filter((s, i) => i === 0 || s).join("\n");
  }
  return row.body ?? "";
}

/** What the field asks for. */
export function prompt(row: CommentRow, mode: Editing["mode"]): string {
  if (mode === "reply") return "返信";
  if (row.kind === "置換") return "置換後の文字列";
  if (row.kind === "挿入") return "挿入する文字列";
  return "コメント";
}
