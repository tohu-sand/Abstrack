<script lang="ts">
  // The handles on the two ends of what the selected comment marks. Dragging
  // one moves that end a character at a time; at the release the comment is
  // changed to mark the new range.
  import type { Caret, CommentRow, PageInfo, Range } from "../lib/backend";
  import { commentRange, isDiscarded, reshapeComment, selectRange } from "../lib/backend";
  import { rectToBox } from "../lib/coords";
  import type { PageSlot } from "../lib/layout";

  interface Props {
    generation: number;
    /** The selected comment, `null` when none is or nothing can be changed. */
    commentKey: string | null;
    /** The comment rows: when they change, the comment may have moved. */
    rows: CommentRow[];
    slots: PageSlot[];
    pages: PageInfo[];
    zoom: number;
    pageLeft: (width: number) => number;
    hidden: boolean;
    onGrab: (e: PointerEvent, fixed: Caret, done: (range: Range) => void) => void;
    onError: (message: string) => void;
  }

  let { generation, commentKey, rows, slots, pages, zoom, pageLeft, hidden, onGrab, onError }: Props = $props();

  interface Marked {
    key: string;
    start: Caret;
    end: Caret;
    page: number;
    /** The rectangles of the first and the last line, user space. */
    first: number[];
    last: number[];
  }

  let marked = $state<Marked | null>(null);
  let asked = 0;

  $effect(() => {
    const key = commentKey;
    const g = generation;
    void rows;
    void find(key, g);
  });

  async function find(key: string | null, g: number) {
    const ask = ++asked;
    if (!key) {
      marked = null;
      return;
    }
    try {
      const range = await commentRange(g, key);
      const selected = range && (await selectRange(g, range.start, range.end));
      if (ask !== asked) return;
      const firstPage = selected?.pages[0];
      const lastPage = selected?.pages[selected.pages.length - 1];
      const first = firstPage?.rects[0];
      const last = lastPage?.rects[lastPage.rects.length - 1];
      marked = range && firstPage && first && last ? { key, ...range, page: firstPage.page, first, last } : null;
    } catch (e) {
      if (ask === asked) marked = null;
      if (!isDiscarded(e)) onError(`ハンドルを表示できませんでした: ${String(e)}`);
    }
  }

  let boxes = $derived.by(() => {
    const m = marked;
    const slot = m && slots[m.page];
    const info = m && pages[m.page];
    if (!m || !slot || !info || hidden) return null;
    const left = pageLeft(slot.width);
    const a = rectToBox(info, zoom, m.first);
    const b = rectToBox(info, zoom, m.last);
    return {
      start: { left: left + a.left, top: slot.top + a.top, height: a.height },
      end: { left: left + b.left + b.width, top: slot.top + b.top, height: b.height },
    };
  });

  function press(which: "start" | "end", e: PointerEvent) {
    const m = marked;
    if (e.button !== 0 || !m) return;
    e.stopPropagation();
    e.preventDefault();
    onGrab(e, which === "start" ? m.end : m.start, (range) => {
      const [from, to] = [range.anchor, range.focus].sort((x, y) => x.page - y.page || x.index - y.index);
      if (same(from!, m.start) && same(to!, m.end)) return;
      reshapeComment(generation, m.key, range).catch((err) => onError(`コメントの範囲を変えられませんでした: ${String(err)}`));
    });
  }

  const same = (a: Caret, b: Caret) => a.page === b.page && a.index === b.index;

  // The press is taken on pointerdown; mousedown must not start a selection.
  const keep = (e: MouseEvent) => {
    e.stopPropagation();
    e.preventDefault();
  };
</script>

{#if boxes}
  <button
    type="button"
    tabindex="-1"
    class="handle start"
    data-key={marked?.key}
    style:left="{boxes.start.left}px"
    style:top="{boxes.start.top}px"
    style:height="{boxes.start.height}px"
    aria-label="範囲の始まり"
    onpointerdown={(e) => press("start", e)}
    onmousedown={keep}
  ></button>
  <button
    type="button"
    tabindex="-1"
    class="handle end"
    data-key={marked?.key}
    style:left="{boxes.end.left}px"
    style:top="{boxes.end.top}px"
    style:height="{boxes.end.height}px"
    aria-label="範囲の終わり"
    onpointerdown={(e) => press("end", e)}
    onmousedown={keep}
  ></button>
{/if}

<style>
  .handle {
    position: absolute;
    z-index: 3;
    width: 12px;
    margin-left: -6px;
    padding: 0;
    border: 0;
    background: transparent;
    cursor: ew-resize;
  }

  .handle::before {
    content: "";
    position: absolute;
    top: 0;
    bottom: 0;
    left: 5px;
    width: 2px;
    background: var(--accent);
  }

  .handle::after {
    content: "";
    position: absolute;
    left: 2px;
    width: 8px;
    height: 8px;
    border-radius: 1px;
    background: var(--accent);
  }

  .start::after {
    top: -7px;
  }

  .end::after {
    bottom: -7px;
  }
</style>
