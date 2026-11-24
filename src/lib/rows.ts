// The rows of the comment list as the window keeps them: a row that did not
// change stays the same object, so only changed rows are drawn again.

import type { CommentRow } from "./backend";

/** `fresh`, with the rows of `old` that did not change put in their place. */
export function keepUnchanged(old: CommentRow[] | null, fresh: CommentRow[]): CommentRow[] {
  if (!old) return fresh;
  const before = new Map(old.map((r) => [r.key, r]));
  return fresh.map((r) => {
    const kept = before.get(r.key);
    return kept && kept.version === r.version ? kept : r;
  });
}

/** Each row's number: its place in reading order, from 1. */
export function numbersOf(rows: CommentRow[] | null): Map<string, number> {
  return new Map((rows ?? []).map((r, i) => [r.key, i + 1]));
}

/** For comparing text: case, and full-width and half-width forms, do not matter. */
export function folded(text: string): string {
  return text.normalize("NFKC").toLowerCase();
}

/** Whether the row's text, what it marks, its author or a reply contains `query` (already folded). */
export function rowMatches(row: CommentRow, query: string): boolean {
  if (!query) return true;
  const fields = [row.body, row.target, row.editText, row.author, ...row.replies.flatMap((r) => [r.text, r.author])];
  return fields.some((f) => f && folded(f).includes(query));
}

/** One line of the row for a message or a list in plain text: what it marks and its text. */
export function rowSummary(row: CommentRow): string {
  const what =
    row.kind === "置換"
      ? `「${row.target}」→「${row.editText ?? ""}」`
      : row.kind === "挿入"
        ? `＋「${row.editText ?? ""}」`
        : row.target
          ? `「${row.target}」`
          : "";
  const body = (row.body ?? "").replace(/\r\n?|\n/g, " ");
  return [row.kind, what, body].filter(Boolean).join(" ");
}

/** The unchecked comments as plain text, one per line: page, number, what and the text. */
export function uncheckedText(rows: CommentRow[], numbers: Map<string, number>, fileName: string): string {
  const open = rows.filter((r) => !r.marked);
  const lines = open.map((r) => `p${r.page + 1} #${numbers.get(r.key)} ${rowSummary(r)}`);
  return [`${fileName} の未対応 ${open.length} 件`, ...lines].join("\n");
}
