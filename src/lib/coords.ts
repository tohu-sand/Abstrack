// The only place that converts between PDF user space and the screen.
//
// User space: points, origin at the bottom-left, as stored in the PDF.
// Page space: CSS pixels from the top-left corner of the page as displayed.
// /Rotate, the offset of the visible box, /UserUnit and the zoom are applied
// here. devicePixelRatio only decides how many bitmap pixels back a CSS pixel
// (see `bitmapSize`).

import type { PageInfo } from "./backend";

/** CSS pixels per point at 100 %: a point is 1/72 inch, a CSS pixel 1/96 inch. */
export const PX_PER_PT = 96 / 72;

export interface Point {
  x: number;
  y: number;
}

export interface Box {
  left: number;
  top: number;
  width: number;
  height: number;
}

export function pageSize(page: PageInfo, zoom: number): { width: number; height: number } {
  return { width: page.width * zoom * PX_PER_PT, height: page.height * zoom * PX_PER_PT };
}

/** User space to display space (points, y up, origin at the visible box's corner). */
function toDisplay(page: PageInfo, x: number, y: number): Point {
  const [x0, y0, x1, y1] = page.visible;
  let p: Point;
  switch (page.rotation) {
    case 90:
      p = { x: y - y0, y: x1 - x };
      break;
    case 180:
      p = { x: x1 - x, y: y1 - y };
      break;
    case 270:
      p = { x: y1 - y, y: x - x0 };
      break;
    default:
      p = { x: x - x0, y: y - y0 };
  }
  return { x: p.x * page.userUnit, y: p.y * page.userUnit };
}

function fromDisplay(page: PageInfo, dx: number, dy: number): Point {
  const [x0, y0, x1, y1] = page.visible;
  const x = dx / page.userUnit;
  const y = dy / page.userUnit;
  switch (page.rotation) {
    case 90:
      return { x: x1 - y, y: x + y0 };
    case 180:
      return { x: x1 - x, y: y1 - y };
    case 270:
      return { x: y + x0, y: y1 - x };
    default:
      return { x: x + x0, y: y + y0 };
  }
}

export function userToPage(page: PageInfo, zoom: number, x: number, y: number): Point {
  const d = toDisplay(page, x, y);
  const s = zoom * PX_PER_PT;
  return { x: d.x * s, y: (page.height - d.y) * s };
}

export function pageToUser(page: PageInfo, zoom: number, px: number, py: number): Point {
  const s = zoom * PX_PER_PT;
  return fromDisplay(page, px / s, page.height - py / s);
}

/** A user-space rectangle [x0, y0, x1, y1] as a box in page space. */
export function rectToBox(page: PageInfo, zoom: number, r: readonly number[]): Box {
  const a = userToPage(page, zoom, r[0]!, r[1]!);
  const b = userToPage(page, zoom, r[2]!, r[3]!);
  const left = Math.min(a.x, b.x);
  const top = Math.min(a.y, b.y);
  return { left, top, width: Math.abs(a.x - b.x), height: Math.abs(a.y - b.y) };
}

/** Corners of a QuadPoints entry (8 numbers) in page space, as an SVG polygon outline. */
export function quadToPolygon(page: PageInfo, zoom: number, q: readonly number[]): Point[] {
  // Order in the file: top-left, top-right, bottom-left, bottom-right.
  const corners = [0, 1, 3, 2].map((k) => userToPage(page, zoom, q[2 * k]!, q[2 * k + 1]!));
  return corners;
}

/** Bitmap size that keeps the page sharp on this screen. */
export function bitmapSize(width: number, height: number, ratio = window.devicePixelRatio || 1) {
  return { width: Math.max(1, Math.round(width * ratio)), height: Math.max(1, Math.round(height * ratio)) };
}
