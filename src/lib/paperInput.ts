// Mouse and keys on the page area: a press and a drag select text (with Alt,
// a rectangle), a drag past the top or bottom edge scrolls, and single keys
// ask for comments.

import type { CommentKind, SelectionDriver } from "./selection";

interface Host {
  driver: SelectionDriver;
  scroller: () => HTMLElement | undefined;
  content: () => HTMLElement | undefined;
  /** Text or a rectangle is selected. */
  hasSelection: () => boolean;
  hasArea: () => boolean;
  setDragging: (dragging: boolean) => void;
  /** A press of the main button on a page, before it selects anything; `target` is the element pressed. */
  onPress: (target: Element | null) => void;
  onCommand: (kind: CommentKind) => void;
}

const COMMAND_KEYS: Record<string, CommentKind> = {
  h: "highlight",
  r: "replace",
  d: "strikeOut",
  u: "underline",
  i: "insert",
  n: "note",
};

const EDGE = 24;

export class PaperInput {
  private lastPointer = { x: 0, y: 0 };
  private autoscroll = 0;

  constructor(private host: Host) {}

  // The press is taken from mousedown rather than pointerdown because only
  // mousedown carries the click count; the pointer is captured on pointerdown
  // so that a drag keeps reporting when it leaves the page area.
  pointerDown = (e: PointerEvent): void => {
    if (e.button !== 0 || onLabel(e)) return;
    this.host.content()?.setPointerCapture(e.pointerId);
  };

  mouseDown = (e: MouseEvent): void => {
    if (e.button !== 0 || onLabel(e)) return;
    e.preventDefault();
    this.host.scroller()?.focus({ preventScroll: true });
    this.lastPointer = { x: e.clientX, y: e.clientY };
    this.host.onPress(document.elementFromPoint(e.clientX, e.clientY));
    this.host.setDragging(true);
    this.host.driver.down(e);
  };

  /** A press on a handle of a comment's range; `start` begins moving that end. */
  grab = (e: PointerEvent, start: () => void): void => {
    this.host.content()?.setPointerCapture(e.pointerId);
    this.host.scroller()?.focus({ preventScroll: true });
    this.lastPointer = { x: e.clientX, y: e.clientY };
    this.host.setDragging(true);
    start();
  };

  pointerMove = (e: PointerEvent): void => {
    this.lastPointer = { x: e.clientX, y: e.clientY };
    this.host.driver.move(e);
    if (this.host.driver.dragging && !this.autoscroll) this.autoscroll = requestAnimationFrame(this.scrollWhileDragging);
  };

  pointerUp = (e: PointerEvent): void => {
    cancelAnimationFrame(this.autoscroll);
    this.autoscroll = 0;
    this.host.setDragging(false);
    // Events of a captured pointer are aimed at the capturing element.
    this.host.driver.up(document.elementFromPoint(e.clientX, e.clientY));
  };

  keyDown = (e: KeyboardEvent): void => {
    const driver = this.host.driver;
    if (e.key === "Escape" && this.host.hasSelection()) {
      e.preventDefault();
      driver.clear();
      return;
    }
    if (driver.key(e)) {
      e.preventDefault();
      return;
    }
    const plain = !e.ctrlKey && !e.altKey && !e.metaKey && !e.shiftKey && !e.isComposing;
    const kind = e.key === "Enter" && this.host.hasArea() ? "area" : plain ? COMMAND_KEYS[e.key.toLowerCase()] : undefined;
    if (kind) {
      e.preventDefault();
      this.host.onCommand(kind);
    }
  };

  // Dragging past the top or bottom edge scrolls, faster the further out.
  private scrollWhileDragging = (): void => {
    this.autoscroll = 0;
    const scroller = this.host.scroller();
    if (!scroller || !this.host.driver.dragging) return;
    const box = scroller.getBoundingClientRect();
    const y = this.lastPointer.y;
    const over = y < box.top + EDGE ? y - (box.top + EDGE) : y > box.bottom - EDGE ? y - (box.bottom - EDGE) : 0;
    if (over !== 0) {
      scroller.scrollTop += Math.max(-40, Math.min(40, over / 2));
      this.host.driver.refresh(this.lastPointer.x, this.lastPointer.y);
    }
    this.autoscroll = requestAnimationFrame(this.scrollWhileDragging);
  };
}

const onLabel = (e: MouseEvent) => e.target instanceof Element && e.target.closest(".label") !== null;
