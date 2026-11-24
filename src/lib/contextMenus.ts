// What a right click offers: the menu bar's commands that apply to what is
// under the pointer, shown as a menu there. The items carry the menu bar's
// ids, so a choice does just what the menu bar item does. They have access
// keys but no shortcuts, as right-click menus on Windows have.

import type { CommentRow, MenuEntry } from "./backend";
import { showContextMenu } from "./backend";

const LINE: MenuEntry = { id: "", text: "" };

/** On selected text: the comments that mark it, and copying it. */
export function textMenu(editable: boolean): MenuEntry[] {
  const copy = { id: "edit.copy", text: "コピー(&C)" };
  if (!editable) return [copy];
  return [
    { id: "comment.highlight", text: "ハイライト(&H)" },
    { id: "comment.replace", text: "置換(&R)" },
    { id: "comment.strikeOut", text: "取り消し線(&D)" },
    { id: "comment.underline", text: "下線(&U)" },
    LINE,
    copy,
  ];
}

/** In a rectangle drawn with Alt+drag. */
export const areaMenu: MenuEntry[] = [{ id: "comment.area", text: "長方形(&A)" }];

/** Anywhere else on a page: what goes in at the place clicked. */
export const placeMenu: MenuEntry[] = [
  { id: "comment.insert", text: "挿入(&I)" },
  { id: "comment.note", text: "メモ(&N)" },
];

/** On a comment, on the page or in the list. */
export function commentMenu(row: CommentRow, editable: boolean): MenuEntry[] {
  const copy =
    row.editText === null
      ? []
      : [{ id: "comment.copyEditText", text: row.kind === "挿入" ? "挿入する文字列をコピー(&T)" : "置換後の文字列をコピー(&T)" }];
  if (!editable) return copy;
  return [
    { id: "comment.edit", text: "コメントを編集(&E)" },
    { id: "comment.reply", text: "返信(&Y)" },
    { id: "comment.check", text: row.marked ? "チェックを外す(&K)" : "チェックを付ける(&K)" },
    ...(copy.length > 0 ? [LINE, ...copy] : []),
    LINE,
    { id: "comment.delete", text: "コメントを削除(&L)" },
  ];
}

/** Shows `entries` at the pointer, unless there are none. */
export function showMenu(entries: MenuEntry[], onError: (message: string) => void): void {
  if (entries.length === 0) return;
  showContextMenu(entries).catch((e) => onError(`メニューを出せませんでした: ${String(e)}`));
}

/** Whether (x, y) lies in the rectangle [x0, y0, x1, y1]. */
export function within(rect: readonly number[], x: number, y: number): boolean {
  const [x0 = 0, y0 = 0, x1 = 0, y1 = 0] = rect;
  return x >= x0 && x <= x1 && y >= y0 && y <= y1;
}
