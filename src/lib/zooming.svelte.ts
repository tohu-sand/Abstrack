// The zoom of the pages: a chosen value, or fitted to the width or to a whole
// page and kept fitted while the window and the list change size.

import { tick } from "svelte";
import type { DocumentInfo } from "./backend";
import { clampZoom } from "./layout";
import { fitZoom, type Fit } from "./view";

/** A place in the document, as (page, fraction across, fraction down). */
export interface Anchor {
  page: number;
  fx: number;
  fy: number;
}

interface Host {
  doc: () => DocumentInfo | null;
  currentPage: () => number;
  anchorAt: (x: number, y: number) => Anchor | null | undefined;
  restoreAnchor: (anchor: Anchor, x: number, y: number) => void;
  scrollToPage: (page: number) => void;
}

type Point = { x: number; y: number };

export class Zooming {
  zoom = $state(1);
  fit = $state<Fit>("width");
  /** The size of the page area, CSS px. */
  viewport = $state({ width: 0, height: 0 });
  /** The document has been laid out at a zoom fitted to a known page area. */
  private placed = false;

  constructor(private host: Host) {
    $effect(() => {
      const doc = host.doc();
      const mode = this.fit;
      const size = this.viewport;
      if (doc && mode && size.width > 0) {
        const z = fitZoom(doc, mode, size);
        // The first document is opened before the page area has a size:
        // there is no place on the paper to keep yet, and it starts at its top.
        if (!this.placed) this.zoom = z;
        else if (Math.abs(z - this.zoom) > 1e-4) this.set(z);
        this.placed = true;
      }
    });
  }

  /** A newly opened document starts fitted to the width. */
  start(doc: DocumentInfo): void {
    this.fit = "width";
    this.zoom = fitZoom(doc, "width", this.viewport);
    this.placed = this.viewport.width > 0;
  }

  /** A chosen zoom, keeping the paper under the point (or the centre) where it was. */
  to(value: number, point?: Point): void {
    this.fit = null;
    this.set(value, point);
  }

  async fitTo(mode: "width" | "page"): Promise<void> {
    this.fit = mode;
    const doc = this.host.doc();
    if (!doc) return;
    const page = this.host.currentPage();
    await this.set(fitZoom(doc, mode, this.viewport));
    // A whole page is shown from its top, as other viewers do.
    if (mode === "page") this.host.scrollToPage(page);
  }

  private async set(value: number, point?: Point): Promise<void> {
    const p = point ?? { x: this.viewport.width / 2, y: this.viewport.height / 2 };
    const anchor = this.host.anchorAt(p.x, p.y);
    this.zoom = clampZoom(value);
    await tick();
    if (anchor) this.host.restoreAnchor(anchor, p.x, p.y);
  }
}
