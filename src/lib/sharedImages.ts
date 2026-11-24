// Page images the application hands over in memory it shares with the page
// (WebView2 only): no bytes cross the IPC channel and nothing is inflated
// here. An image arrives as an event carrying the number it was asked for
// with; its memory stays mapped until it is released.

interface SharedBufferEvent extends Event {
  getBuffer(): ArrayBuffer;
  additionalData?: { image?: number };
}

interface WebViewHost {
  addEventListener(type: "sharedbufferreceived", listener: (event: SharedBufferEvent) => void): void;
  releaseBuffer(buffer: ArrayBuffer): void;
}

const host = (window as { chrome?: { webview?: Partial<WebViewHost> } }).chrome?.webview;
const waiting = new Map<number, (buffer: ArrayBuffer) => void>();
let last = 0;

if (host?.addEventListener && host.releaseBuffer) {
  host.addEventListener("sharedbufferreceived", (event) => {
    const buffer = event.getBuffer();
    const token = event.additionalData?.image;
    const take = token === undefined ? undefined : waiting.get(token);
    if (take) {
      waiting.delete(token!);
      take(buffer);
    } else {
      // Asked for by a request that was given up meanwhile.
      releaseSharedImage(buffer);
    }
  });
}

export interface Expected {
  /** Passed with the request; the image is announced with it. */
  token: number;
  arrived: Promise<ArrayBuffer>;
  /** The request ended without an image: one that still arrives is released. */
  forget: () => void;
}

/** Prepares to receive an image; `null` where the web view shares no memory. */
export function expectSharedImage(): Expected | null {
  if (!host?.addEventListener || !host.releaseBuffer) return null;
  const token = (last = (last % 0x7fffffff) + 1);
  const arrived = new Promise<ArrayBuffer>((resolve) => waiting.set(token, resolve));
  return { token, arrived, forget: () => waiting.delete(token) };
}

/** Unmaps an image; its buffer cannot be read afterwards. */
export function releaseSharedImage(buffer: ArrayBuffer): void {
  host?.releaseBuffer?.(buffer);
}
