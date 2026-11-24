// Dealing with the comments one by one: the check mark (Acrobat's) and
// moving to the next unchecked comment in reading order.

import * as api from "./backend";
import type { CommentRow, DocumentInfo } from "./backend";
import { uncheckedText } from "./rows";
import { abbreviate } from "./selection";

interface Host {
  doc: () => DocumentInfo | null;
  rows: () => CommentRow[] | null;
  numbers: () => Map<string, number>;
  /** The rows the list shows now. */
  shown: () => CommentRow[];
  selectedKey: () => string | null;
  currentPage: () => number;
  /** Selects the row and shows the comment on the page and in the list. */
  go: (key: string) => void;
  message: (text: string) => void;
  error: (text: string) => void;
}

export class Checking {
  constructor(private host: Host) {}

  async toggle(key: string | null): Promise<void> {
    const doc = this.host.doc();
    const row = key === null ? undefined : this.host.rows()?.find((r) => r.key === key);
    if (!doc || !row) return;
    if (doc.readOnlyReason) {
      this.host.error(doc.readOnlyReason);
      return;
    }
    try {
      await api.setCommentMarked(doc.generation, row.key, !row.marked);
      const number = this.host.numbers().get(row.key);
      this.host.message(row.marked ? `#${number} のチェックを外しました` : `#${number} をチェックしました`);
    } catch (e) {
      this.host.error(`${row.marked ? "チェックを外せませんでした" : "チェックできませんでした"}: ${String(e)}`);
    }
  }

  /**
   * Moves to the next (`step` 1) or previous (-1) unchecked comment in reading
   * order among the rows shown: from the selected one, or from the page in
   * view when none is. Past the last it goes round to the first.
   */
  next(step: 1 | -1): void {
    const rows = this.host.rows();
    if (!rows) return;
    const numbers = this.host.numbers();
    const n = (r: CommentRow) => numbers.get(r.key) ?? 0;
    const open = this.host.shown().filter((r) => !r.marked);
    open.sort((a, b) => n(a) - n(b));
    if (open.length === 0) {
      const hidden = rows.filter((r) => !r.marked).length;
      this.host.message(
        hidden > 0 ? `表示中の未対応はありません。絞り込みで隠れている未対応が ${hidden} 件あります` : "未対応のコメントはありません",
      );
      return;
    }
    const key = this.host.selectedKey();
    const from = key === null ? undefined : numbers.get(key);
    const after = (r: CommentRow) => (from === undefined ? r.page >= this.host.currentPage() : n(r) > from);
    const before = (r: CommentRow) => (from === undefined ? r.page <= this.host.currentPage() : n(r) < from);
    let target = step > 0 ? open.find(after) : [...open].reverse().find(before);
    if (!target) {
      target = step > 0 ? open[0]! : open[open.length - 1]!;
      if (target.key === key) this.host.message(`未対応はこの #${n(target)} だけです`);
      else this.host.message(step > 0 ? "最初の未対応に戻りました" : "最後の未対応に移りました");
    }
    this.host.go(target.key);
  }

  /** Copies the replacement or insertion text of a row, to paste into the manuscript. */
  async copyEditText(row: CommentRow): Promise<void> {
    if (row.editText === null) return;
    try {
      await navigator.clipboard.writeText(row.editText);
      this.host.message(`「${abbreviate(row.editText)}」をコピーしました`);
    } catch (e) {
      this.host.error(`クリップボードに書けませんでした: ${String(e)}`);
    }
  }

  /** Copies the unchecked comments as plain text, one per line. */
  async copyUnchecked(): Promise<void> {
    const doc = this.host.doc();
    const rows = this.host.rows();
    if (!doc || !rows) return;
    const count = rows.filter((r) => !r.marked).length;
    try {
      await navigator.clipboard.writeText(uncheckedText(rows, this.host.numbers(), doc.fileName));
      this.host.message(`未対応 ${count} 件の一覧をコピーしました`);
    } catch (e) {
      this.host.error(`クリップボードに書けませんでした: ${String(e)}`);
    }
  }
}
