// Saving, undoing and copying as the menu and their keys ask for them. A
// text field with the focus comes first: its typing is finished before a
// save, and Ctrl+Z and Ctrl+C act on its text.

import { tick } from "svelte";
import * as api from "./backend";
import type { DocumentInfo } from "./backend";

interface Host {
  doc: () => DocumentInfo | null;
  /** The text selected on the page. */
  selectedText: () => string;
  /** Copies what the comment list offers while it has the keyboard; whether it did. */
  copyFromList: () => boolean;
  message: (text: string) => void;
  error: (text: string) => void;
}

const typing = (): boolean =>
  document.activeElement instanceof HTMLTextAreaElement || document.activeElement instanceof HTMLInputElement;

/** Ends typing in a comment's field, which keeps the text. */
async function finishTyping(): Promise<void> {
  if (typing()) (document.activeElement as HTMLElement).blur();
  await tick();
}

export class EditCommands {
  constructor(private host: Host) {}

  save = (): void => {
    void finishTyping().then(() => api.saveDocument().catch((e) => this.host.error(String(e))));
  };

  saveAs = (): void => {
    void finishTyping().then(() => api.saveDocumentAs().catch((e) => this.host.error(String(e))));
  };

  async undoOrRedo(redo: boolean): Promise<void> {
    if (typing()) {
      document.execCommand(redo ? "redo" : "undo");
      return;
    }
    const doc = this.host.doc();
    if (!doc) return;
    try {
      const what = await api.undoChange(doc.generation, redo);
      if (what) this.host.message(`${redo ? "やり直しました" : "元に戻しました"}: ${what}`);
    } catch (e) {
      this.host.error(`${redo ? "やり直せませんでした" : "元に戻せませんでした"}: ${String(e)}`);
    }
  }

  copy = (): void => {
    if (typing()) {
      document.execCommand("copy");
      return;
    }
    if (this.host.copyFromList()) return;
    const text = this.host.selectedText();
    if (!text) return;
    // The PDF selection lives outside the DOM, so supply its plain text to the native copy operation.
    let supplied = false;
    const fill = (event: ClipboardEvent) => {
      if (!event.clipboardData) return;
      event.clipboardData.setData("text/plain", text);
      event.preventDefault();
      supplied = true;
    };
    document.addEventListener("copy", fill);
    try {
      if (document.execCommand("copy") && supplied) return;
    } finally {
      document.removeEventListener("copy", fill);
    }
    void this.writeClipboard(text);
  };

  private async writeClipboard(text: string): Promise<void> {
    try {
      await navigator.clipboard.writeText(text);
    } catch (e) {
      this.host.error(`クリップボードに書けませんでした: ${String(e)}`);
    }
  }
}
