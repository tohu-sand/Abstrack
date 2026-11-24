// Text cut off at the edge of a narrow element can still be read whole as its
// tooltip. Text shown whole gets none, so that pointing at it does not put
// the same words over the window a second time.

import type { Attachment } from "svelte/attachments";

/** `{@attach titleIfCut(text)}`: `text` becomes the tooltip only while the element cuts it off. */
export function titleIfCut(text: string): Attachment<HTMLElement> {
  return (el) => {
    const check = () => {
      if (el.scrollWidth > el.clientWidth) el.title = text;
      else el.removeAttribute("title");
    };
    el.addEventListener("mouseenter", check);
    return () => {
      el.removeEventListener("mouseenter", check);
      el.removeAttribute("title");
    };
  };
}
