// Text selection by pointer and keyboard. Positions are sent to the
// application, which owns the text model; this module only turns input into
// caret positions and keeps the latest answer.

import type { Caret, CaretUnit, CommentRequest, SelectionInfo } from "./backend";
import { caretAt, isDiscarded, moveCaret, selectRange, unitAt } from "./backend";

export interface PagePoint {
  page: number;
  /** User space. */
  x: number;
  y: number;
}

export interface Selection extends SelectionInfo {
  anchor: Caret;
  focus: Caret;
}

export interface Area {
  page: number;
  /** User space [x0, y0, x1, y1]. */
  rect: [number, number, number, number];
}

interface Options {
  generation: () => number;
  pointAt: (clientX: number, clientY: number) => PagePoint | null;
  /** The selected ranges, the one being made or changed last. */
  onSelection: (ranges: Selection[]) => void;
  onArea: (area: Area | null) => void;
  /** A press and release without movement; `target` is the element under the pointer. */
  onClick: (target: Element | null) => void;
  onError: (message: string) => void;
}

interface TextDrag {
  kind: "text";
  epoch: number;
  anchor: Promise<Caret | null>;
  x: number;
  y: number;
  moved: boolean;
  /** Started with the key that adds a range: a press without a drag changes nothing. */
  adding: boolean;
  /** The latest pointer position not yet turned into a selection. */
  pending: PagePoint | null;
  busy: boolean;
  /** Settles once the pointer positions given so far are selected. */
  following: Promise<void>;
  /** Moving an end of a comment's range: takes the range at the release. */
  done?: (range: { anchor: Caret; focus: Caret }) => void;
}

interface AreaDrag {
  kind: "area";
  start: PagePoint;
  x: number;
  y: number;
  moved: boolean;
  adding: false;
}

const DRAG_THRESHOLD = 3;

export type CommentKind = "highlight" | "replace" | "strikeOut" | "underline" | "insert" | "note" | "area";

/** The name of each kind of comment and the key that makes it. */
export const KIND_LABELS: Record<CommentKind, [string, string]> = {
  highlight: ["ハイライト", "H"],
  replace: ["置換", "R"],
  strikeOut: ["取り消し線", "D"],
  underline: ["下線", "U"],
  insert: ["挿入", "I"],
  note: ["メモ", "N"],
  area: ["長方形", "Enter"],
};

const KEY_STEPS: Record<string, [CaretUnit, CaretUnit, boolean]> = {
  ArrowRight: ["char", "word", true],
  ArrowLeft: ["char", "word", false],
  ArrowDown: ["line", "line", true],
  ArrowUp: ["line", "line", false],
};

/** Ctrl (Command on a Mac) with a press adds a range to the selection. */
const addsRange = (e: MouseEvent) => (/Mac/.test(navigator.platform) ? e.metaKey : e.ctrlKey);

export class SelectionDriver {
  /** The range being made or changed. */
  current: Selection | null = null;
  /** Ranges added before it with Ctrl+drag. */
  private kept: Selection[] = [];
  /**
   * The ends of the selection, kept when it shrinks to nothing; after a plain
   * click both are where it put the caret. Shift+click and Shift+arrow keys
   * go on from here.
   */
  private ends: { anchor: Caret; focus: Caret } | null = null;
  /** Where the main button was last pressed on a page: the place of a note. */
  lastPoint: PagePoint | null = null;
  private drag: TextDrag | AreaDrag | null = null;
  /** Bumped by every press and by clearing, so that answers to older input are dropped. */
  private epoch = 0;
  private keys: Promise<void> = Promise.resolve();

  constructor(private opts: Options) {}

  /**
   * A press of the main button. `e.detail` is the click count (2: word,
   * 3: sentence). With Ctrl (Command on a Mac) the new range is added to the
   * ranges already selected.
   */
  down(e: MouseEvent): void {
    const point = this.opts.pointAt(e.clientX, e.clientY);
    if (!point) return;
    this.lastPoint = point;
    const epoch = ++this.epoch;
    if (e.altKey) {
      this.deselect();
      this.drag = { kind: "area", start: point, x: e.clientX, y: e.clientY, moved: false, adding: false };
      return;
    }
    const adding = addsRange(e);
    if (adding) {
      if (this.current) this.kept.push(this.current);
      this.current = null;
      this.ends = null;
    } else if (!(e.shiftKey && this.ends)) {
      this.kept = [];
    }
    if (e.detail >= 2) {
      this.drag = null;
      void this.selectUnit(epoch, point, e.detail === 2 ? "word" : "sentence");
      return;
    }
    const from = e.shiftKey && !adding ? (this.ends?.anchor ?? null) : null;
    if (from) {
      const drag = this.textDrag(epoch, Promise.resolve(from), e);
      drag.moved = true;
      drag.pending = point;
      void this.follow(drag);
      return;
    }
    const anchor = this.locate(point);
    this.drag = this.textDrag(epoch, anchor, e);
    this.drag.adding = adding;
    void anchor.then((c) => {
      if (c && epoch === this.epoch) this.ends = { anchor: c, focus: c };
    });
  }

  move(e: PointerEvent): void {
    const drag = this.drag;
    if (!drag) return;
    if (!drag.moved && Math.hypot(e.clientX - drag.x, e.clientY - drag.y) < DRAG_THRESHOLD) return;
    drag.moved = true;
    const point = this.opts.pointAt(e.clientX, e.clientY);
    if (!point) return;
    if (drag.kind === "area") {
      if (point.page === drag.start.page) {
        const a = drag.start;
        this.opts.onArea({
          page: a.page,
          rect: [Math.min(a.x, point.x), Math.min(a.y, point.y), Math.max(a.x, point.x), Math.max(a.y, point.y)],
        });
      }
      return;
    }
    drag.pending = point;
    void this.follow(drag);
  }

  /**
   * A press on the handle at one end of a comment's range: the selection
   * becomes the range from `fixed`, the other end, to the pointer, and at the
   * release `done` takes it.
   */
  grab(e: MouseEvent, fixed: Caret, done: (range: { anchor: Caret; focus: Caret }) => void): void {
    const epoch = ++this.epoch;
    this.kept = [];
    this.current = null;
    this.ends = null;
    const drag = this.textDrag(epoch, Promise.resolve(fixed), e);
    drag.moved = true;
    drag.done = done;
  }

  /** The release of the button; `target` is the element under the pointer. */
  up(target: Element | null): void {
    const drag = this.drag;
    this.drag = null;
    if (drag?.kind === "text" && drag.done) {
      const done = drag.done;
      void drag.following.then(() => {
        if (drag.epoch !== this.epoch) return;
        const ends = this.ends;
        this.clear();
        if (ends && compare(ends.anchor, ends.focus) !== 0) done(ends);
      });
      return;
    }
    if (drag && !drag.moved && !drag.adding) this.opts.onClick(target);
  }

  get dragging(): boolean {
    return this.drag?.moved ?? false;
  }

  /** Re-reads the caret under the pointer, e.g. after scrolling during a drag. */
  refresh(clientX: number, clientY: number): void {
    const drag = this.drag;
    if (drag?.kind !== "text" || !drag.moved) return;
    const point = this.opts.pointAt(clientX, clientY);
    if (point) {
      drag.pending = point;
      void this.follow(drag);
    }
  }

  /**
   * Shift+arrow keys move the end of the selection by a character (with Ctrl,
   * a word) or a line. Returns whether the key is one of them. Presses are
   * handled in turn, so that quick repeats each take effect.
   */
  key(e: KeyboardEvent): boolean {
    const step = KEY_STEPS[e.key];
    if (!step || !e.shiftKey || !this.ends) return false;
    const unit = e.ctrlKey ? step[1] : step[0];
    const forward = step[2];
    this.keys = this.keys.then(async () => {
      const ends = this.ends;
      if (!ends) return;
      const epoch = this.epoch;
      // The end of a selection made forwards sits after its last character.
      const after = compare(ends.anchor, ends.focus) <= 0;
      try {
        const focus = await moveCaret(this.opts.generation(), ends.focus, unit, forward, after);
        await this.select(epoch, ends.anchor, focus);
      } catch (err) {
        this.fail(err);
      }
    });
    return true;
  }

  /**
   * Selects from `anchor` to `focus` without the pointer (text found with the
   * search, for one); `point` becomes the place of a note.
   */
  async selectRange(anchor: Caret, focus: Caret, point: PagePoint): Promise<void> {
    const epoch = ++this.epoch;
    this.drag = null;
    this.kept = [];
    this.lastPoint = point;
    await this.select(epoch, anchor, focus);
  }

  /**
   * For a right click away from the selection: the selection goes, and the
   * caret and the place of a note move to `point`, as a click there does.
   * Whether no other input came first.
   */
  async caretTo(point: PagePoint): Promise<boolean> {
    const epoch = ++this.epoch;
    this.drag = null;
    this.lastPoint = point;
    this.deselect();
    const at = await this.locate(point);
    if (epoch !== this.epoch) return false;
    this.ends = at ? { anchor: at, focus: at } : null;
    return true;
  }

  /** Clears the selection, the rectangle and the caret. */
  clear(): void {
    this.epoch++;
    this.deselect();
    this.ends = null;
  }

  /** Clears the selection and the rectangle, keeping the caret of the last click. */
  deselect(): void {
    this.current = null;
    this.kept = [];
    this.opts.onSelection([]);
    this.opts.onArea(null);
  }

  /**
   * Where an insertion goes: the end of the selection, or where the last
   * click put the caret. `after`: at a line break, the end of the earlier line.
   */
  get insertionPoint(): { at: Caret; after: boolean } | null {
    if (!this.ends) return null;
    return { at: this.ends.focus, after: compare(this.ends.anchor, this.ends.focus) <= 0 };
  }

  /**
   * What a comment of `kind` would mark now: the selected text, the caret,
   * the place last clicked or the rectangle. A message saying what to do
   * first when there is nothing to mark.
   */
  request(kind: CommentKind, area: Area | null): CommentRequest | string {
    switch (kind) {
      case "highlight":
      case "replace":
      case "strikeOut":
      case "underline": {
        const ranges = this.ranges.map((r) => ({ anchor: r.anchor, focus: r.focus }));
        return ranges.length > 0 ? { kind, ranges } : "先に文字を選んでください";
      }
      case "insert": {
        const p = this.insertionPoint;
        return p ? { kind, at: p.at, after: p.after } : "先に挿入する位置をクリックしてください";
      }
      case "note": {
        const p = this.lastPoint;
        return p ? { kind, page: p.page, x: p.x, y: p.y } : "先にメモを置く位置をクリックしてください";
      }
      case "area":
        return area ? { kind, page: area.page, rect: area.rect } : "先に Alt+ドラッグで囲んでください";
    }
  }

  /** All selected ranges, the one being made or changed last. */
  get ranges(): Selection[] {
    return this.current ? [...this.kept, this.current] : [...this.kept];
  }

  private textDrag(epoch: number, anchor: Promise<Caret | null>, e: MouseEvent): TextDrag {
    const drag: TextDrag = {
      kind: "text", epoch, anchor, x: e.clientX, y: e.clientY, moved: false, adding: false, pending: null, busy: false,
      following: Promise.resolve(),
    };
    this.drag = drag;
    return drag;
  }

  /** Turns the drag's latest pointer position into a selection, one request at a time. */
  private follow(drag: TextDrag): Promise<void> {
    if (drag.busy) return drag.following;
    drag.busy = true;
    drag.following = (async () => {
      try {
        while (drag.pending && drag.epoch === this.epoch) {
          const point = drag.pending;
          drag.pending = null;
          const [anchor, focus] = await Promise.all([drag.anchor, this.locate(point)]);
          if (anchor && focus) await this.select(drag.epoch, anchor, focus);
        }
      } finally {
        drag.busy = false;
      }
    })();
    return drag.following;
  }

  private async selectUnit(epoch: number, point: PagePoint, unit: CaretUnit): Promise<void> {
    const at = await this.locate(point);
    if (!at || epoch !== this.epoch) return;
    try {
      const [start, end] = await unitAt(this.opts.generation(), at, unit);
      await this.select(epoch, start, end);
    } catch (err) {
      this.fail(err);
    }
  }

  private async locate(point: PagePoint): Promise<Caret | null> {
    try {
      return await caretAt(this.opts.generation(), point.page, point.x, point.y);
    } catch (err) {
      this.fail(err);
      return null;
    }
  }

  private async select(epoch: number, anchor: Caret, focus: Caret): Promise<void> {
    if (epoch !== this.epoch) return;
    try {
      const info = await selectRange(this.opts.generation(), anchor, focus);
      if (epoch !== this.epoch) return;
      this.current = info.text.length > 0 ? { ...info, anchor, focus } : null;
      this.ends = { anchor, focus };
      if (this.current) this.opts.onArea(null);
      this.opts.onSelection(this.ranges);
    } catch (err) {
      this.fail(err);
    }
  }

  private fail(err: unknown): void {
    if (!isDiscarded(err)) this.opts.onError(`選択できませんでした: ${String(err)}`);
  }
}

function compare(a: Caret, b: Caret): number {
  return a.page - b.page || a.index - b.index;
}

/** The selected text for the status bar: its start and end, with the middle left out. */
export function abbreviate(text: string, keep = 14): string {
  const flat = text.replace(/\s+/g, " ").trim();
  const chars = [...flat];
  if (chars.length <= keep * 2 + 1) return flat;
  return `${chars.slice(0, keep).join("")}…${chars.slice(-keep).join("")}`;
}
