// Tells the application when the page has gone without input for a while, so
// that it can give memory back, and when input returns.

import { invoke } from "@tauri-apps/api/core";

const IDLE_AFTER_MS = 60_000;
const EVENTS = ["pointerdown", "pointermove", "wheel", "keydown"] as const;

export function watchIdle(onError: (message: string) => void): () => void {
  let idle = false;
  let timer = 0;

  const report = (value: boolean) => {
    idle = value;
    invoke("set_page_idle", { idle: value }).catch((e) =>
      onError(`待機中かどうかを伝えられませんでした: ${String(e)}`),
    );
  };

  const arm = () => {
    window.clearTimeout(timer);
    timer = window.setTimeout(() => report(true), IDLE_AFTER_MS);
  };

  const onInput = () => {
    if (idle) report(false);
    arm();
  };

  for (const e of EVENTS) window.addEventListener(e, onInput, { passive: true, capture: true });
  arm();
  return () => {
    window.clearTimeout(timer);
    for (const e of EVENTS) window.removeEventListener(e, onInput, { capture: true });
  };
}
