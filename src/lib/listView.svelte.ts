// What the comment list shows: its order, a search and filters. Moving to
// the next unchecked comment follows the same filters, so that it never
// stops on a row the list hides.

import type { CommentRow } from "./backend";
import { folded, rowMatches } from "./rows";

export type Order = "reading" | "date" | "author";
export type Status = "all" | "unchecked" | "checked";

export class ListView {
  order = $state<Order>("reading");
  query = $state("");
  status = $state<Status>("all");
  /** "" for every row, `author:<name>` or `kind:<label>`. */
  narrow = $state("");

  get filtering(): boolean {
    return this.query.trim() !== "" || this.status !== "all" || this.narrow !== "";
  }

  /** In page order the list names each page once, above its rows. */
  get byPage(): boolean {
    return this.order === "reading";
  }

  /** The rows shown, in the list's order. */
  shown(rows: CommentRow[] | null, numbers: Map<string, number>): CommentRow[] {
    if (!rows) return [];
    const query = folded(this.query.trim());
    const [by, value] = splitNarrow(this.narrow);
    const out = rows.filter(
      (r) =>
        (this.status === "all" || r.marked === (this.status === "checked")) &&
        (by !== "author" || r.author === value) &&
        (by !== "kind" || r.kind === value) &&
        rowMatches(r, query),
    );
    const n = (r: CommentRow) => numbers.get(r.key) ?? 0;
    if (this.order === "date") out.sort((a, b) => a.timestamp - b.timestamp || n(a) - n(b));
    if (this.order === "author") out.sort((a, b) => a.author.localeCompare(b.author, "ja") || n(a) - n(b));
    return out;
  }

  /** Back to every row; the order stays. */
  clear(): void {
    this.query = "";
    this.status = "all";
    this.narrow = "";
  }
}

/** The rows of each page, for rows in page order. */
export function pageGroups(rows: CommentRow[]): { page: number; rows: CommentRow[] }[] {
  const groups: { page: number; rows: CommentRow[] }[] = [];
  for (const row of rows) {
    const last = groups[groups.length - 1];
    if (last?.page === row.page) last.rows.push(row);
    else groups.push({ page: row.page, rows: [row] });
  }
  return groups;
}

function splitNarrow(narrow: string): ["author" | "kind" | "", string] {
  const at = narrow.indexOf(":");
  const by = narrow.slice(0, at);
  return by === "author" || by === "kind" ? [by, narrow.slice(at + 1)] : ["", ""];
}

/** Choices for narrowing the list: the authors and the kinds the rows have. */
export function narrowChoices(rows: CommentRow[] | null): { authors: string[]; kinds: string[] } {
  const authors = [...new Set((rows ?? []).map((r) => r.author))].sort((a, b) => a.localeCompare(b, "ja"));
  const kinds = [...new Set((rows ?? []).map((r) => r.kind))];
  return { authors, kinds };
}
