// Searching the text of the document: the query, the matches found and the
// one shown.

import * as api from "./backend";
import type { DocumentInfo, SearchHit } from "./backend";

interface Host {
  doc: () => DocumentInfo | null;
  currentPage: () => number;
  /** Puts the focus in the search field once it is shown. */
  focusField: () => void;
  focusPaper: () => void;
  /** Selects the text of a match on the page (and gives the page the keyboard). */
  selectHit: (hit: SearchHit) => void;
  scrollToArea: (page: number, rect: number[]) => void;
  error: (text: string) => void;
}

export class Searching {
  open = $state(false);
  query = $state("");
  shown = $state.raw<SearchHit | undefined>();
  /** Cumulative match counts in page order. Only pages being read need rectangles. */
  private ends = $state.raw<number[]>([]);
  total = $derived(this.ends.at(-1) ?? 0);
  index = $state(0);
  busy = $state(false);
  /** The search for the current query has finished. */
  done = $state(false);
  private request = 0;
  private timer = 0;
  private target = 0;
  private matches = new Map<number, Promise<SearchHit[]>>();

  constructor(private host: Host) {}

  show(): void {
    if (!this.host.doc()) return;
    if (!this.open) {
      this.open = true;
      if (this.query) void this.run();
    }
    queueMicrotask(() => this.host.focusField());
  }

  /** Closes the search; the match shown stays selected, ready for a comment key. */
  close(): void {
    const shown = this.shown;
    const query = this.query;
    this.open = false;
    this.reset();
    this.query = query;
    if (shown) this.host.selectHit(shown);
    else this.host.focusPaper();
  }

  reset(): void {
    this.request++;
    window.clearTimeout(this.timer);
    this.timer = 0;
    this.query = "";
    this.shown = undefined;
    this.ends = [];
    this.matches.clear();
    this.index = 0;
    this.target = 0;
    this.busy = false;
    this.done = false;
  }

  input(query: string): void {
    this.reset();
    this.query = query;
    if (query.trim()) this.timer = window.setTimeout(() => this.run(), 120);
  }

  private async run(): Promise<void> {
    window.clearTimeout(this.timer);
    this.timer = 0;
    const query = this.query;
    const doc = this.host.doc();
    if (!doc || !this.open || !query.trim()) return;
    const request = this.request;
    const origin = this.host.currentPage();
    this.busy = true;
    try {
      const ends = new Array<number>(doc.pages.length);
      // Start on the page being read. Short batches let new input stop the scan.
      for (const [start, limit] of [[origin, doc.pages.length], [0, origin]] as const) {
        for (let page = start; page < limit;) {
          const end = Math.min(limit, page + (page === origin ? 1 : 16));
          const batch = await api.searchCounts(doc.generation, query, page, end);
          if (this.request !== request) return;
          for (let i = 0; i < batch.length; i++) ends[page + i] = batch[i]!;
          const first = this.shown ? -1 : batch.findIndex(n => n > 0);
          if (first >= 0) {
            const hits = await this.pageMatches(page + first);
            if (this.request !== request) return;
            this.showHit(hits[0]!);
          }
          page = end;
        }
      }
      for (let p = 1; p < ends.length; p++) ends[p] = ends[p - 1]! + ends[p]!;
      this.ends = ends;
      this.index = this.shown ? ends[this.shown.page - 1] ?? 0 : 0;
      this.target = this.index;
      this.done = true;
    } catch (e) {
      if (this.request === request && !api.isDiscarded(e)) this.host.error(`検索できませんでした: ${String(e)}`);
    } finally {
      if (this.request === request) this.busy = false;
    }
  }

  /** The next (or previous) match, going round; opens the search when there are none. */
  step(direction: 1 | -1): void {
    const n = this.total;
    if (this.timer) void this.run();
    else if (this.done && n > 0) void this.go((this.target + direction + n) % n);
    else if (!this.open) this.show();
  }

  hasMatches(page: number): boolean {
    return (this.ends[page] ?? 0) > (this.ends[page - 1] ?? 0);
  }

  pageMatches(page: number): Promise<SearchHit[]> {
    const cached = this.matches.get(page);
    if (cached) return cached;
    const doc = this.host.doc();
    if (!doc) return Promise.resolve([]);
    const hits = api.search(doc.generation, this.query, page, page + 1).catch(e => {
      if (this.matches.get(page) === hits) this.matches.delete(page);
      throw e;
    });
    this.matches.set(page, hits);
    if (this.matches.size > 4) this.matches.delete(this.matches.keys().next().value!);
    return hits;
  }

  private async go(index: number): Promise<void> {
    const request = this.request;
    this.target = index;
    let lo = 0;
    let hi = this.ends.length - 1;
    while (lo < hi) {
      const mid = (lo + hi) >>> 1;
      if (this.ends[mid]! <= index) lo = mid + 1;
      else hi = mid;
    }
    try {
      const hits = await this.pageMatches(lo);
      if (request !== this.request || this.target !== index) return;
      this.index = index;
      this.showHit(hits[index - (this.ends[lo - 1] ?? 0)]!);
    } catch (e) {
      if (request === this.request && this.target === index) {
        this.target = this.index;
        if (!api.isDiscarded(e)) this.host.error(`検索できませんでした: ${String(e)}`);
      }
    }
  }

  private showHit(hit: SearchHit): void {
    this.shown = hit;
    const first = hit.rects[0];
    if (first) this.host.scrollToArea(hit.page, first);
  }
}
