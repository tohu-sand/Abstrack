// Page bitmaps arrive as large buffers that the JavaScript engine counts as
// external memory and collects late. Once drawing has been quiet for a moment,
// a collection is asked for (the window starts the engine with --expose-gc).

const QUIET_MS = 300;
let timer = 0;

export function collectSoon(): void {
  const gc = (globalThis as { gc?: () => void }).gc;
  if (typeof gc !== "function") return;
  window.clearTimeout(timer);
  timer = window.setTimeout(() => gc(), QUIET_MS);
}
