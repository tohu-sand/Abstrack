<script lang="ts">
  import type { CommentRow, PageInfo, SearchHit } from "../lib/backend";
  import type { Searching } from "../lib/searching.svelte";
  import { isDiscarded, renderPage, renderRegion } from "../lib/backend";
  import { damageBetween } from "../lib/damage";
  import { bitmapSize, quadToPolygon, rectToBox } from "../lib/coords";
  import { collectSoon } from "../lib/gc";

  interface Props {
    generation: number;
    index: number;
    info: PageInfo;
    zoom: number;
    width: number;
    height: number;
    preview: boolean;
    comments: CommentRow[];
    numbers: Map<string, number>;
    selectedKey: string | null;
    flashKey: string | null;
    search: Searching;
    /** Selected text on this page: one user-space rectangle per line. */
    selected: number[][];
    /** A rectangle chosen with Alt+drag, user space. */
    area: number[] | null;
    /** Bumped whenever the page's annotations change: the page is drawn again. */
    revision: number;
    onSelect: (key: string) => void;
    onError: (message: string) => void;
    /** The page's picture is on its canvas. */
    onPainted: () => void;
  }

  let {
    generation,
    index,
    info,
    zoom,
    width,
    height,
    preview,
    comments,
    numbers,
    selectedKey,
    flashKey,
    search,
    selected,
    area,
    revision,
    onSelect,
    onError,
    onPainted,
  }: Props = $props();

  let canvas: HTMLCanvasElement | undefined = $state();
  let ratio = $state(window.devicePixelRatio || 1);
  /** Height of a number label, CSS px; matches `.comment-number` in base.css. */
  const LABEL_H = 16;
  /** As much of a picture as goes onto a canvas within one frame without holding the frame up. */
  const FRAME_BYTES = 16 * 1024 * 1024;

  let pageHits = $state.raw<SearchHit[]>([]);
  let currentFound = $derived(search.shown?.page === index ? search.shown : undefined);
  let found = $derived(pageHits.length ? pageHits : currentFound ? [currentFound] : []);
  $effect(() => {
    pageHits = [];
    if (!search.hasMatches(index)) return;
    let cancelled = false;
    search.pageMatches(index).then(hits => {
      if (!cancelled) pageHits = hits;
    }).catch(e => {
      if (!cancelled && !isDiscarded(e)) onError(`検索できませんでした: ${String(e)}`);
    });
    return () => { cancelled = true; };
  });

  // devicePixelRatio fires no event when it changes; a resolution query for the
  // current ratio stops matching instead, and the effect then watches the new one.
  $effect(() => {
    const query = window.matchMedia(`(resolution: ${ratio}dppx)`);
    const update = () => (ratio = window.devicePixelRatio || 1);
    query.addEventListener("change", update);
    return () => query.removeEventListener("change", update);
  });

  // Draw the page at the bitmap size for the current zoom and screen. When
  // its annotations change, only where it looks different is drawn again over
  // the drawing shown. Pages drawn recently come back from the application's
  // compressed cache. While the zoom changes, the previous bitmap stays,
  // stretched by CSS, and the new one is asked for once the zoom has settled.
  $effect(() => {
    const target = canvas;
    if (!target) return;
    const full = bitmapSize(width, height, ratio);
    const fullKey = `${generation}:${full.width}x${full.height}`;
    // Keep an already sharp page sharp. New pages crossed at speed only need
    // enough pixels to recognize the text layout and figures.
    const scale = preview && target.dataset.key !== fullKey ? Math.min(1, 768 / Math.max(full.width, full.height)) : 1;
    const size = scale < 1 ? bitmapSize(full.width, full.height, scale) : full;
    const drawing = revision;
    const key = `${generation}:${size.width}x${size.height}`;
    const same = target.dataset.key === key;
    const shown = same ? Number(target.dataset.revision) : NaN;
    if (shown === drawing) return;
    const changed = same ? damageBetween(generation, index, shown, drawing) : null;
    let cancelled = false;
    const resized = target.dataset.painted && target.dataset.fullKey !== fullKey;
    const timer = window.setTimeout(
      async () => {
        try {
          if (changed) await patch(target, size, changed, () => cancelled);
          else {
            const image = await renderPage(generation, index, size.width, size.height, () => cancelled);
            if (!image) return;
            try {
              await show(target, new ImageData(new Uint8ClampedArray(image.rgba), size.width, size.height), () => cancelled);
            } finally {
              image.release();
            }
          }
          if (cancelled) return;
          target.dataset.key = key;
          target.dataset.fullKey = fullKey;
          target.dataset.revision = String(drawing);
          onPainted();
        } catch (error) {
          if (!cancelled && !isDiscarded(error)) onError(`${index + 1} ページを表示できませんでした: ${String(error)}`);
        }
      },
      resized ? 120 : 0,
    );
    return () => {
      cancelled = true;
      window.clearTimeout(timer);
    };
  });

  /** Draws the parts of the page around `rects` (user space) again, over the drawing shown. */
  async function patch(target: HTMLCanvasElement, size: { width: number; height: number }, rects: number[][], cancelled: () => boolean) {
    const [sx, sy] = [size.width / width, size.height / height];
    const boxes: number[][] = [];
    for (const r of rects) {
      const b = rectToBox(info, zoom, r);
      // A pixel more on each side for the antialiased edge.
      let box = [Math.floor(b.left * sx) - 1, Math.floor(b.top * sy) - 1, Math.ceil((b.left + b.width) * sx) + 1, Math.ceil((b.top + b.height) * sy) + 1];
      box = [Math.max(0, box[0]!), Math.max(0, box[1]!), Math.min(size.width, box[2]!), Math.min(size.height, box[3]!)];
      // Overlapping boxes, as before and after a small change, are drawn once.
      const overlapping = boxes.find((o) => o[0]! < box[2]! && box[0]! < o[2]! && o[1]! < box[3]! && box[1]! < o[3]!);
      if (overlapping) overlapping.splice(0, 4, Math.min(overlapping[0]!, box[0]!), Math.min(overlapping[1]!, box[1]!), Math.max(overlapping[2]!, box[2]!), Math.max(overlapping[3]!, box[3]!));
      else if (box[2]! > box[0]! && box[3]! > box[1]!) boxes.push(box);
    }
    for (const [x0, y0, x1, y1] of boxes as [number, number, number, number][]) {
      const region = { x: x0, y: y0, w: x1 - x0, h: y1 - y0 };
      const buffer = await renderRegion(generation, index, size, region);
      if (cancelled()) return;
      context(target)?.putImageData(new ImageData(new Uint8ClampedArray(buffer), region.w, region.h), region.x, region.y);
    }
    collectSoon();
  }

  function context(target: HTMLCanvasElement): CanvasRenderingContext2D | null {
    const ctx = target.getContext("2d", { alpha: false });
    if (!ctx) onError("Canvas 2D が使えないため、ページを表示できません");
    return ctx;
  }

  /** The next frame, or a moment later where none is being made (a hidden window). */
  function nextFrame(): Promise<void> {
    return new Promise((resolve) => {
      const late = window.setTimeout(resolve, 100);
      requestAnimationFrame(() => {
        window.clearTimeout(late);
        resolve();
      });
    });
  }

  /**
   * Puts the picture of the whole page on its canvas. A page that is not on
   * screen yet gets a large picture a part per frame: writing all of it at
   * once takes longer than a frame, and would hold up the very scrolling
   * that brings the page in.
   */
  async function show(target: HTMLCanvasElement, image: ImageData, cancelled: () => boolean) {
    if (target.width !== image.width || target.height !== image.height) {
      target.width = image.width;
      target.height = image.height;
    }
    const ctx = context(target);
    if (!ctx) return;
    const box = target.getBoundingClientRect();
    const onScreen = box.bottom > 0 && box.top < window.innerHeight;
    const rows = onScreen ? image.height : Math.max(1, Math.floor(FRAME_BYTES / (image.width * 4)));
    for (let y = 0; y < image.height; y += rows) {
      if (y > 0) await nextFrame();
      if (cancelled()) return;
      ctx.putImageData(image, 0, 0, 0, y, image.width, Math.min(rows, image.height - y));
    }
    target.dataset.painted = "true";
  }

  interface Mark {
    row: CommentRow;
    box: { left: number; top: number; width: number; height: number };
    polygons: string[];
    label: { x: number; y: number };
  }

  // Number labels go in the margin on the side of the annotation and are
  // stacked downwards when they would overlap.
  let marks: Mark[] = $derived.by(() => {
    const out: Mark[] = [];
    const lastBottom = { left: -Infinity, right: -Infinity };
    const sorted = [...comments].sort((a, b) => b.rect[3] - a.rect[3]);
    for (const row of sorted) {
      const box = rectToBox(info, zoom, row.rect);
      const polygons = row.quads.map((q) =>
        quadToPolygon(info, zoom, q)
          .map((p) => `${p.x.toFixed(1)},${p.y.toFixed(1)}`)
          .join(" "),
      );
      const side = box.left + box.width / 2 < width / 2 ? "left" : "right";
      const y = Math.max(box.top, lastBottom[side] + 1);
      lastBottom[side] = y + LABEL_H;
      const x = side === "left" ? 3 : width - 3;
      out.push({ row, box, polygons, label: { x, y } });
    }
    return out;
  });
</script>

<div class="page" style:width="{width}px" style:height="{height}px">
  <canvas bind:this={canvas} style:width="{width}px" style:height="{height}px"></canvas>
  <svg class="overlay" {width} {height} viewBox="0 0 {width} {height}" aria-hidden="true">
    {#each selected as r, i (i)}
      {@const b = rectToBox(info, zoom, r)}
      <rect class="selection" x={b.left} y={b.top} width={b.width} height={b.height} />
    {/each}
    {#if area}
      {@const b = rectToBox(info, zoom, area)}
      <rect class="area" x={b.left} y={b.top} width={b.width} height={b.height} />
    {/if}
    {#each found as hit, i (i)}
      {#each hit.rects as r, j (j)}
        {@const b = rectToBox(info, zoom, r)}
        <rect class="found" class:current={hit.start === currentFound?.start && hit.end === currentFound.end} x={b.left} y={b.top} width={b.width} height={b.height} />
      {/each}
    {/each}
    {#each marks as m (m.row.key)}
      {@const selected = m.row.key === selectedKey}
      {@const flashing = m.row.key === flashKey}
      <!-- A click here selects the comment (the page area tells a click from a
           drag); the comment list offers the same from the keyboard. -->
      <g class="hit" class:selected class:flashing data-key={m.row.key} style:--annotation-color={m.row.color}>
        {#if m.polygons.length > 0}
          {#each m.polygons as points, i (i)}
            <polygon {points} />
          {/each}
        {:else}
          <rect x={m.box.left} y={m.box.top} width={Math.max(m.box.width, 4)} height={Math.max(m.box.height, 4)} />
        {/if}
      </g>
    {/each}
  </svg>
  {#each marks as m (m.row.key)}
    <button
      type="button"
      class="label comment-number"
      style:--annotation-color={m.row.color}
      class:selected={m.row.key === selectedKey}
      class:right={m.label.x > width / 2}
      data-key={m.row.key}
      style:top="{m.label.y}px"
      style:left={m.label.x > width / 2 ? "auto" : `${m.label.x}px`}
      style:right={m.label.x > width / 2 ? `${width - m.label.x}px` : "auto"}
      tabindex="-1"
      onclick={() => onSelect(m.row.key)}>#{numbers.get(m.row.key)}</button
    >
  {/each}
</div>

<style>
  .page {
    position: relative;
    background: #fff;
    outline: 1px solid var(--line);
  }

  canvas {
    position: absolute;
    inset: 0;
    display: block;
  }

  .overlay {
    position: absolute;
    inset: 0;
    overflow: visible;
  }

  .selection {
    fill: color-mix(in srgb, var(--accent) 32%, transparent);
    pointer-events: none;
  }

  .area {
    fill: color-mix(in srgb, var(--accent) 10%, transparent);
    stroke: var(--accent);
    stroke-width: 1;
    stroke-dasharray: 4 3;
    pointer-events: none;
  }

  .found {
    fill: color-mix(in srgb, var(--accent) 22%, transparent);
    pointer-events: none;
  }

  .found.current {
    stroke: var(--accent);
    stroke-width: 2;
  }

  .hit polygon,
  .hit rect {
    fill: transparent;
    stroke: none;
    cursor: pointer;
  }

  .hit.selected polygon,
  .hit.selected rect {
    stroke: color-mix(in srgb, var(--annotation-color, #888) 65%, #252525);
    stroke-width: 1;
  }

  .hit.flashing polygon,
  .hit.flashing rect {
    stroke: color-mix(in srgb, var(--annotation-color, #888) 65%, #252525);
    stroke-width: 3;
    fill: color-mix(in srgb, var(--annotation-color, #888) 12%, transparent);
  }

  .label {
    position: absolute;
    cursor: pointer;
  }

  .label.selected {
    outline: 1px solid #303030;
    outline-offset: 1px;
  }
</style>
