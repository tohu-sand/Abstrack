// The comment list's place in the window: shown or not, its width and its
// type size, kept in the settings.

import * as api from "./backend";

export class ListPanel {
  visible = $state(true);
  width = $state(360);
  fontSize = $state(13);

  constructor(private error: (text: string) => void) {}

  async load(): Promise<void> {
    try {
      const s = await api.uiSettings();
      this.visible = s.listVisible;
      this.width = s.listWidth;
      this.fontSize = s.listFontSize;
    } catch (e) {
      this.error(`表示の設定を読み込めませんでした: ${String(e)}`);
    }
  }

  toggle(): void {
    this.visible = !this.visible;
    this.save();
  }

  setFont(size: number): void {
    this.fontSize = Math.min(24, Math.max(10, size));
    this.save();
  }

  /** Dragging the splitter between the page area and the list. */
  startResize(event: PointerEvent): void {
    const handle = event.currentTarget as HTMLElement;
    handle.setPointerCapture(event.pointerId);
    const move = (e: PointerEvent) => {
      this.width = Math.min(900, Math.max(200, Math.round(window.innerWidth - e.clientX)));
    };
    const end = () => {
      handle.removeEventListener("pointermove", move);
      handle.removeEventListener("pointerup", end);
      this.save();
    };
    handle.addEventListener("pointermove", move);
    handle.addEventListener("pointerup", end);
  }

  private save(): void {
    api
      .setUiSettings({ listVisible: this.visible, listWidth: this.width, listFontSize: this.fontSize })
      .catch((e) => this.error(`表示の設定を保存できませんでした: ${String(e)}`));
  }
}
