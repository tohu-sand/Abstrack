// Vertical layout of the pages in the scrolling paper area, in CSS px.

import type { PageInfo } from "./backend";
import { pageSize } from "./coords";

export const PAGE_GAP = 12;
export const PAPER_PADDING = 12;

export interface PageSlot {
  top: number;
  width: number;
  height: number;
}

export interface Layout {
  slots: PageSlot[];
  totalHeight: number;
  maxWidth: number;
}

export function layoutPages(pages: PageInfo[], zoom: number): Layout {
  let y = PAPER_PADDING;
  let maxWidth = 0;
  const slots = pages.map((p) => {
    const size = pageSize(p, zoom);
    const slot = { top: y, width: size.width, height: size.height };
    y += size.height + PAGE_GAP;
    maxWidth = Math.max(maxWidth, size.width);
    return slot;
  });
  return { slots, totalHeight: y - PAGE_GAP + PAPER_PADDING, maxWidth };
}

/** Index of the last page whose top is at or above `y` (0 when above the first page). */
export function pageAt(layout: Layout, y: number): number {
  const slots = layout.slots;
  let lo = 0;
  let hi = slots.length - 1;
  while (lo < hi) {
    const mid = (lo + hi + 1) >> 1;
    if (slots[mid]!.top <= y) lo = mid;
    else hi = mid - 1;
  }
  return Math.max(0, lo);
}

export const ZOOM_STEPS = [0.25, 0.33, 0.5, 0.67, 0.75, 0.9, 1, 1.1, 1.25, 1.5, 1.75, 2, 2.5, 3, 4];
export const MIN_ZOOM = 0.1;
export const MAX_ZOOM = 6;

export function nextZoom(zoom: number, direction: 1 | -1): number {
  if (direction > 0) return ZOOM_STEPS.find((z) => z > zoom + 1e-3) ?? MAX_ZOOM;
  return [...ZOOM_STEPS].reverse().find((z) => z < zoom - 1e-3) ?? MIN_ZOOM;
}

export function clampZoom(zoom: number): number {
  return Math.min(MAX_ZOOM, Math.max(MIN_ZOOM, zoom));
}
