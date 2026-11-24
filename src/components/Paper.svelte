<script lang="ts">
  import { onDestroy, untrack } from "svelte";
  import type { CommentRow, DocumentInfo, SearchHit } from "../lib/backend";
  import type { Searching } from "../lib/searching.svelte";
  import { prefetchPages, setVisiblePages } from "../lib/backend";
  import { bitmapSize, pageToUser, rectToBox } from "../lib/coords";
  import { clampZoom, layoutPages, pageAt, PAPER_PADDING } from "../lib/layout";
  import { SelectionDriver, type Area, type CommentKind, type PagePoint, type Selection } from "../lib/selection";
  import type { CommentRequest } from "../lib/backend";
  import PageView from "./PageView.svelte";
  import RangeHandles from "./RangeHandles.svelte";
  import SelectionActions from "./SelectionActions.svelte";
  import { PaperInput } from "../lib/paperInput";
  import { areaMenu, commentMenu, placeMenu, showMenu, textMenu, within } from "../lib/contextMenus";

  interface Props {
    doc: DocumentInfo;
    rows: CommentRow[];
    numbers: Map<string, number>;
    zoom: number;
    selectedKey: string | null;
    flashKey: string | null;
    search: Searching;
    onSelect: (key: string) => void;
    /** The selected comment is let go: a press on a page away from it. */
    onDeselect: () => void;
    onError: (message: string) => void;
    /** A page's picture came on screen. */
    onPainted: () => void;
    onCurrentPage: (page: number) => void;
    onViewport: (size: { width: number; height: number }) => void;
    onZoomRequest: (zoom: number, anchor: { x: number; y: number }) => void;
    onSelectionChange: (ranges: Selection[]) => void;
    onAreaChange: (area: Area | null) => void;
    /** A key or a button asked for a comment of this kind. */
    onCommand: (kind: CommentKind) => void;
  }

  let {
    doc,
    rows,
    numbers,
    zoom,
    selectedKey,
    flashKey,
    search,
    onSelect,
    onDeselect,
    onError,
    onPainted,
    onCurrentPage,
    onViewport,
    onZoomRequest,
    onSelectionChange,
    onAreaChange,
    onCommand,
  }: Props = $props();

  let scroller: HTMLDivElement | undefined = $state();
  let content: HTMLDivElement | undefined = $state();
  let selection = $state<Selection[]>([]);
  let area = $state<Area | null>(null);
  let selectionByPage = $derived.by(() => {
    const m = new Map<number, number[][]>();
    for (const p of selection.flatMap((s) => s.pages)) m.set(p.page, [...(m.get(p.page) ?? []), ...p.rects]);
    return m;
  });
  let viewport = $state({ width: 0, height: 0 });
  let scroll = $state({ top: 0, left: 0 });

  let layout = $derived(layoutPages(doc.pages, zoom));
  let contentWidth = $derived(Math.max(viewport.width, layout.maxWidth + 2 * PAPER_PADDING));
  // clientWidth is rounded; at fractional device pixel ratios a width set to it
  // can exceed the real inner width by a fraction and bring up a scroll bar.
  let overflowing = $derived(layout.maxWidth + 2 * PAPER_PADDING > viewport.width);
  // During a fling, mount extra previews on the side that is coming into view.
  let fastDirection = $state<-1 | 0 | 1>(0);
  let firstVisible = $derived(pageAt(layout, scroll.top - viewport.height * (fastDirection < 0 ? 2 : 0.5)));
  let lastVisible = $derived.by(() => {
    const bottom = scroll.top + viewport.height * (fastDirection > 0 ? 3 : 1.5);
    let last = firstVisible;
    while (last + 1 < layout.slots.length && layout.slots[last + 1]!.top < bottom) last++;
    return last;
  });
  // No page is shown before the page area has a size: the zoom fitted to it
  // is not known until then, and a page drawn at another would be thrown away.
  let visible = $derived(
    layout.slots.length && viewport.height > 0 ? Array.from({ length: lastVisible - firstVisible + 1 }, (_, i) => firstVisible + i) : [],
  );
  let ahead = $derived(
    fastDirection || visible.length === 0
      ? []
      : [visible[0]! - 1, visible[visible.length - 1]! + 1].filter((p) => p >= 0 && p < layout.slots.length),
  );
  let rowsByPage = $derived.by(() => {
    const m = new Map<number, CommentRow[]>();
    for (const r of rows) {
      const list = m.get(r.page);
      if (list) list.push(r);
      else m.set(r.page, [r]);
    }
    return m;
  });

  $effect(() => {
    const pages = [...visible, ...ahead];
    const generation = doc.generation;
    const next = ahead.map((page) => {
      const slot = layout.slots[page]!;
      return { page, ...bitmapSize(slot.width, slot.height) };
    });
    setVisiblePages(pages)
      .then(() => next.length ? prefetchPages(generation, next) : undefined)
      .catch((e) => onError(`ページを表示できませんでした: ${String(e)}`));
  });

  let centrePage = $derived(pageAt(layout, scroll.top + viewport.height / 2));
  $effect(() => {
    const page = centrePage;
    untrack(() => onCurrentPage(page));
  });

  $effect(() => {
    const el = scroller;
    if (!el) return;
    // Measured at once as well: the observer first reports a frame later,
    // and the first page would be asked for that much later.
    const measure = () => {
      if (el.clientWidth === viewport.width && el.clientHeight === viewport.height) return;
      viewport = { width: el.clientWidth, height: el.clientHeight };
      onViewport(viewport);
    };
    measure();
    const observer = new ResizeObserver(measure);
    observer.observe(el);
    return () => observer.disconnect();
  });

  let lastScroll = 0;
  let settleTimer = 0;
  onDestroy(() => window.clearTimeout(settleTimer));
  function onScroll() {
    if (!scroller) return;
    const top = scroller.scrollTop;
    const distance = top - scroll.top;
    if (distance) {
      const now = performance.now();
      const elapsed = Math.max(8, now - lastScroll);
      lastScroll = now;
      if (Math.abs(distance) > viewport.height * 0.4 || Math.abs(distance) / elapsed > viewport.height / 300) {
        fastDirection = distance > 0 ? 1 : -1;
        window.clearTimeout(settleTimer);
        settleTimer = window.setTimeout(() => (fastDirection = 0), 90);
      }
    }
    scroll = { top, left: scroller.scrollLeft };
  }

  function pageLeft(width: number): number {
    return Math.max(PAPER_PADDING, Math.floor((contentWidth - width) / 2));
  }

  // Ctrl+wheel zooms around the pointer.
  function onWheel(event: WheelEvent) {
    if (!event.ctrlKey || !scroller) return;
    event.preventDefault();
    const rect = scroller.getBoundingClientRect();
    const factor = Math.exp(-event.deltaY * 0.0015);
    onZoomRequest(clampZoom(zoom * factor), { x: event.clientX - rect.left, y: event.clientY - rect.top });
  }

  $effect(() => {
    const el = scroller;
    if (!el) return;
    el.addEventListener("wheel", onWheel, { passive: false });
    return () => el.removeEventListener("wheel", onWheel);
  });

  /** Where a point of the viewport sits in the document, as (page, fraction across, fraction down). */
  export function anchorAt(x: number, y: number) {
    const docY = (scroller?.scrollTop ?? 0) + y;
    const docX = (scroller?.scrollLeft ?? 0) + x;
    const page = pageAt(layout, docY);
    const slot = layout.slots[page];
    if (!slot) return null;
    return {
      page,
      fx: (docX - pageLeft(slot.width)) / slot.width,
      fy: (docY - slot.top) / slot.height,
    };
  }

  /** Scrolls so that the anchor ends up under viewport point (x, y) after a re-layout. */
  export function restoreAnchor(anchor: { page: number; fx: number; fy: number }, x: number, y: number) {
    const slot = layout.slots[anchor.page];
    if (!slot || !scroller) return;
    scroller.scrollTop = slot.top + anchor.fy * slot.height - y;
    scroller.scrollLeft = pageLeft(slot.width) + anchor.fx * slot.width - x;
  }

  let animation = 0;
  function scrollTo(top: number, left: number) {
    const el = scroller;
    if (!el) return;
    cancelAnimationFrame(animation);
    // A long way is covered at once but for the last stretch: pages passed
    // on the way would otherwise each be drawn for a frame or two.
    const reach = el.clientHeight / 2;
    if (Math.abs(top - el.scrollTop) > reach) el.scrollTop = top - Math.sign(top - el.scrollTop) * reach;
    const startTop = el.scrollTop;
    const startLeft = el.scrollLeft;
    const started = performance.now();
    const duration = 110;
    const step = (now: number) => {
      const t = Math.min(1, (now - started) / duration);
      const ease = 1 - (1 - t) * (1 - t);
      el.scrollTop = startTop + (top - startTop) * ease;
      el.scrollLeft = startLeft + (left - startLeft) * ease;
      if (t < 1) animation = requestAnimationFrame(step);
    };
    animation = requestAnimationFrame(step);
  }

  /** Shows the top of the document at once: where a newly opened one starts. */
  export function toTop() {
    cancelAnimationFrame(animation);
    if (scroller) {
      scroller.scrollTop = 0;
      scroller.scrollLeft = 0;
    }
    scroll = { top: 0, left: 0 };
  }

  export function scrollToPage(page: number) {
    const slot = layout.slots[page];
    if (slot) scrollTo(slot.top - PAPER_PADDING, scroller?.scrollLeft ?? 0);
  }

  /** Brings a comment into view, a third of the way down the viewport. */
  export function scrollToComment(row: CommentRow) {
    scrollToArea(row.page, row.rect);
  }

  /** Brings a user-space rectangle of a page into view, a third of the way down. */
  export function scrollToArea(page: number, rect: readonly number[]) {
    const slot = layout.slots[page];
    const info = doc.pages[page];
    if (!slot || !info || !scroller) return;
    const box = rectToBox(info, zoom, rect);
    const top = slot.top + box.top - viewport.height / 3;
    const boxLeft = pageLeft(slot.width) + box.left;
    let left = scroller.scrollLeft;
    if (boxLeft < left || boxLeft + box.width > left + viewport.width) left = boxLeft - viewport.width / 4;
    scrollTo(Math.max(0, top), Math.max(0, left));
  }

  export function focus() {
    scroller?.focus();
  }

  export function clearSelection() {
    driver.clear();
  }

  /** Selects text found with the search, and gives the page area the keyboard. */
  export function selectFound(hit: SearchHit) {
    const first = hit.rects[0];
    if (!first) return;
    scroller?.focus({ preventScroll: true });
    void driver.selectRange(
      { page: hit.page, index: hit.start },
      { page: hit.page, index: hit.end },
      { page: hit.page, x: first[0]!, y: first[3]! },
    );
  }

  /** What a comment of `kind` would mark now, or what to do first. */
  export function commentRequest(kind: CommentKind): CommentRequest | string {
    return driver.request(kind, area);
  }

  let dragging = $state(false);
  let editable = $derived(!doc.readOnlyReason);

  // The small row of comment buttons just below the end of the selection,
  // or below the rectangle.
  let actions = $derived.by(() => {
    if (dragging || !editable) return null;
    const last = selection[selection.length - 1];
    const target = last
      ? last.pages[last.pages.length - 1]
      : area
        ? { page: area.page, rects: [area.rect] }
        : undefined;
    const rect = target?.rects[target.rects.length - 1];
    const slot = target && layout.slots[target.page];
    const info = target && doc.pages[target.page];
    if (!target || !rect || !slot || !info) return null;
    const box = rectToBox(info, zoom, rect);
    const kinds: CommentKind[] = last ? ["highlight", "replace", "strikeOut", "underline"] : ["area"];
    return { left: pageLeft(slot.width) + box.left, top: slot.top + box.top + box.height + 4, kinds };
  });

  /** The page under a point of the window, in user space. */
  function pointAt(clientX: number, clientY: number): PagePoint | null {
    if (!content) return null;
    const box = content.getBoundingClientRect();
    const y = clientY - box.top;
    const page = pageAt(layout, y);
    const slot = layout.slots[page];
    const info = doc.pages[page];
    if (!slot || !info) return null;
    const p = pageToUser(info, zoom, clientX - box.left - pageLeft(slot.width), y - slot.top);
    return { page, x: p.x, y: p.y };
  }

  const driver = new SelectionDriver({
    generation: () => doc.generation,
    pointAt,
    onSelection: (s) => {
      selection = s;
      onSelectionChange(s);
    },
    onArea: (a) => {
      area = a;
      onAreaChange(a);
    },
    onClick: (target) => {
      driver.deselect();
      const key = target?.closest<HTMLElement>("[data-key]")?.dataset.key;
      if (key) onSelect(key);
    },
    onError: (message) => onError(message),
  });

  const input = new PaperInput({
    driver,
    scroller: () => scroller,
    content: () => content,
    hasSelection: () => selection.length > 0 || area !== null,
    hasArea: () => area !== null,
    setDragging: (d) => (dragging = d),
    // A press on the selected comment keeps it, so that clicking it does not
    // flicker; its handles take their own presses.
    onPress: (target) => {
      const key = target?.closest<HTMLElement>("[data-key]")?.dataset.key;
      if (selectedKey && key !== selectedKey) onDeselect();
    },
    onCommand: (kind) => onCommand(kind),
  });

  // A right click shows the menu bar's commands for what is under the
  // pointer: the selected text or rectangle, a comment, or else the place
  // itself, where the caret moves as a click would move it.
  async function onContextMenu(e: MouseEvent) {
    e.preventDefault();
    const point = e.button === 2 ? pointAt(e.clientX, e.clientY) : null;
    if (!point) return;
    scroller?.focus({ preventScroll: true });
    const hit = (rects: readonly number[][]) => rects.some((r) => within(r, point.x, point.y));
    const key = document.elementFromPoint(e.clientX, e.clientY)?.closest<HTMLElement>("[data-key]")?.dataset.key;
    const row = key === undefined ? undefined : rows.find((r) => r.key === key);
    if (selection.some((s) => s.pages.some((p) => p.page === point.page && hit(p.rects)))) {
      showMenu(textMenu(editable), onError);
    } else if (area?.page === point.page && hit([area.rect])) {
      showMenu(editable ? areaMenu : [], onError);
    } else if (row) {
      driver.deselect();
      onSelect(row.key);
      showMenu(commentMenu(row, editable), onError);
    } else {
      if (selectedKey) onDeselect();
      if ((await driver.caretTo(point)) && editable) showMenu(placeMenu, onError);
    }
  }
</script>

<!-- The page area takes focus so that it can be scrolled from the keyboard and
     a text selection can be extended with Shift+arrow keys. -->
<!-- svelte-ignore a11y_no_noninteractive_tabindex, a11y_no_noninteractive_element_interactions -->
<div
  class="paper"
  bind:this={scroller}
  onscroll={onScroll}
  onkeydown={input.keyDown}
  tabindex="0"
  role="region"
  aria-label="文書"
>
  <div
    class="content"
    bind:this={content}
    style:height="{layout.totalHeight}px"
    style:width={overflowing ? `${contentWidth}px` : "100%"}
    onpointerdown={input.pointerDown}
    onmousedown={input.mouseDown}
    onpointermove={input.pointerMove}
    onpointerup={input.pointerUp}
    onpointercancel={input.pointerUp}
    oncontextmenu={onContextMenu}
    role="presentation"
  >
    {#each visible as index (index)}
      {@const slot = layout.slots[index]}
      {@const info = doc.pages[index]}
      {#if slot && info}
        <div class="slot" style:top="{slot.top}px" style:left="{pageLeft(slot.width)}px">
          <PageView
            generation={doc.generation}
            {index}
            {info}
            {zoom}
            width={slot.width}
            height={slot.height}
            preview={fastDirection !== 0}
            comments={rowsByPage.get(index) ?? []}
            {numbers}
            {selectedKey}
            {flashKey}
            {search}
            selected={selectionByPage.get(index) ?? []}
            area={area?.page === index ? area.rect : null}
            revision={doc.pageRevisions[index] ?? 0}
            {onSelect}
            {onError}
            {onPainted}
          />
        </div>
      {/if}
    {/each}
    {#if actions}
      <SelectionActions left={actions.left} top={actions.top} kinds={actions.kinds} onPick={onCommand} />
    {/if}
    <RangeHandles
      generation={doc.generation}
      commentKey={editable ? selectedKey : null}
      {rows}
      slots={layout.slots}
      pages={doc.pages}
      {zoom}
      {pageLeft}
      hidden={dragging}
      onGrab={(e, fixed, done) => input.grab(e, () => driver.grab(e, fixed, done))}
      {onError}
    />
  </div>
</div>

<style>
  .paper {
    position: relative;
    overflow-x: auto;
    /* Always present, so that its width does not change the fit and the layout. */
    overflow-y: scroll;
    background: var(--bg-canvas);
    outline: none;
  }

  .paper:focus-visible {
    box-shadow: inset 0 0 0 1px var(--accent);
  }

  .content {
    position: relative;
    cursor: text;
    touch-action: pan-x pan-y;
  }

  .slot {
    position: absolute;
  }
</style>
