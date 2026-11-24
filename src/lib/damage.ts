// Where pages look different after each change to their annotations, kept by
// page revision for a while: a page shown at an earlier revision is drawn
// again only there.

/** User space [x0, y0, x1, y1]. */
type Rect = number[];

const KEEP = 32;
let generation = -1;
const byPage = new Map<number, Map<number, Rect[]>>();

/** Records rectangles of `page` that look different at `revision`. */
export function recordDamage(doc: number, page: number, revision: number, rect: Rect): void {
  if (doc !== generation) {
    generation = doc;
    byPage.clear();
  }
  let revisions = byPage.get(page);
  if (!revisions) byPage.set(page, (revisions = new Map()));
  revisions.set(revision, [...(revisions.get(revision) ?? []), rect]);
  if (revisions.size > KEEP) revisions.delete(revisions.keys().next().value!);
}

/** What changed on `page` after revision `from` up to `to`; `null` when not all of it is known. */
export function damageBetween(doc: number, page: number, from: number, to: number): Rect[] | null {
  if (doc !== generation || !(from < to)) return null;
  const revisions = byPage.get(page);
  const out: Rect[] = [];
  for (let r = from + 1; r <= to; r++) {
    const rects = revisions?.get(r);
    if (!rects) return null;
    out.push(...rects);
  }
  return out;
}
