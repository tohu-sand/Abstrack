import type { DocumentInfo } from "./backend";
import { PX_PER_PT } from "./coords";
import { clampZoom, PAPER_PADDING } from "./layout";

export type Fit = "width" | "page" | null;

/** Zoom at which the widest page fits the width (or the tallest the height too). */
export function fitZoom(doc: DocumentInfo, mode: "width" | "page", viewport: { width: number; height: number }): number {
  let maxWidth = 0;
  let maxHeight = 0;
  for (const p of doc.pages) {
    maxWidth = Math.max(maxWidth, p.width);
    maxHeight = Math.max(maxHeight, p.height);
  }
  if (maxWidth === 0 || viewport.width === 0) return 1;
  const byWidth = (viewport.width - 2 * PAPER_PADDING - 2) / (maxWidth * PX_PER_PT);
  if (mode === "width") return clampZoom(byWidth);
  const byHeight = (viewport.height - 2 * PAPER_PADDING - 2) / (maxHeight * PX_PER_PT);
  return clampZoom(Math.min(byWidth, byHeight));
}
