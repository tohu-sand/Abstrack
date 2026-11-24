// What the status bar asks about until it is answered: the open file changed
// or taken away by another program, and changes that a run which ended
// abruptly did not save.

import * as api from "./backend";
import type { DocumentInfo, Outside, RecoveryOffer } from "./backend";

export interface NoticeAction {
  label: string;
  /** Only what the notice and the label do not show. */
  title?: string;
  run: () => void;
}

export interface Notice {
  text: string;
  actions: NoticeAction[];
}

interface Host {
  doc: () => DocumentInfo | null;
  currentPage: () => number;
  error: (text: string) => void;
}

export class Notices {
  outside = $state<Outside>("same");
  offers = $state<RecoveryOffer[]>([]);
  /** The page to come back to after reading the file again. */
  private returnTo: { path: string; page: number } | null = null;

  constructor(private host: Host) {}

  current = $derived.by((): Notice | null => {
    const doc = this.host.doc();
    if (doc && this.outside === "changed") {
      return {
        text: `${doc.fileName} が別のアプリで変更されました。`,
        actions: [{ label: "再読み込み", run: () => this.reload() }],
      };
    }
    if (doc && this.outside === "missing") {
      return {
        text: `${doc.fileName} が元の場所にありません。保存するときは「名前を付けて保存」を使ってください。`,
        actions: [],
      };
    }
    const offer = this.offers[0];
    if (!offer) return null;
    return {
      text: `${offer.written}、${offer.fileName} の変更が保存されずに終わりました`,
      actions: [
        { label: "復元", title: offer.path, run: () => this.act(api.restoreRecovery(offer.id)) },
        { label: "破棄", run: () => this.act(api.discardRecovery(offer.id)) },
      ],
    };
  });

  async loadOffers(): Promise<void> {
    try {
      this.offers = await api.recoveryOffers();
    } catch (e) {
      this.host.error(`戻せる変更を調べられませんでした: ${String(e)}`);
    }
  }

  /** A document was opened (or none is): the page to show first. */
  opened(doc: DocumentInfo | null): number {
    this.outside = doc?.outside ?? "same";
    const back = this.returnTo;
    this.returnTo = null;
    return doc && back?.path === doc.path ? Math.min(back.page, doc.pages.length - 1) : 0;
  }

  reported(generation: number, outside: Outside): void {
    if (generation === this.host.doc()?.generation) this.outside = outside;
  }

  private reload(): void {
    const doc = this.host.doc();
    if (!doc) return;
    this.returnTo = { path: doc.path, page: this.host.currentPage() };
    this.act(api.reloadDocument());
  }

  private act(call: Promise<void>): void {
    call.catch((e) => this.host.error(String(e)));
  }
}
