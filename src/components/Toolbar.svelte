<script lang="ts">
  import type { Fit } from "../lib/view";

  interface Props {
    zoom: number;
    fit: Fit;
    pageCount: number;
    currentPage: number;
    hasOutline: boolean;
    listVisible: boolean;
    onFit: (fit: "width" | "page") => void;
    onZoom: (zoom: number) => void;
    onZoomStep: (direction: 1 | -1) => void;
    onGoToPage: (page: number) => void;
    onOutline: () => void;
    onToggleList: () => void;
  }

  let {
    zoom,
    fit,
    pageCount,
    currentPage,
    hasOutline,
    listVisible,
    onFit,
    onZoom,
    onZoomStep,
    onGoToPage,
    onOutline,
    onToggleList,
  }: Props = $props();

  const PRESETS = [0.5, 0.75, 1, 1.25, 1.5, 2, 3, 4];
  let pageInput: HTMLInputElement | undefined = $state();
  let pageText = $state("");

  $effect(() => {
    pageText = String(currentPage + 1);
  });

  let choice = $derived(fit ?? (PRESETS.some((p) => Math.abs(p - zoom) < 1e-3) ? String(zoom) : "custom"));

  function onChoice(event: Event) {
    const value = (event.currentTarget as HTMLSelectElement).value;
    if (value === "width" || value === "page") onFit(value);
    else if (value !== "custom") onZoom(Number(value));
  }

  function onPageKey(event: KeyboardEvent) {
    if (event.key === "Enter") {
      const n = Number.parseInt(pageText, 10);
      if (Number.isFinite(n)) onGoToPage(Math.min(pageCount, Math.max(1, n)) - 1);
      pageText = String(currentPage + 1);
      pageInput?.blur();
    } else if (event.key === "Escape") {
      pageText = String(currentPage + 1);
      pageInput?.blur();
    }
  }

  export function focusPage() {
    pageInput?.focus();
    pageInput?.select();
  }
</script>

<div class="toolbar" role="toolbar" aria-label="ツールバー">
  <div class="group">
    <button type="button" onclick={() => onZoomStep(-1)} title="Ctrl+Shift+-">縮小</button>
    <select value={choice} onchange={onChoice} aria-label="表示倍率">
      <option value="width">幅に合わせる</option>
      <option value="page">ページ全体</option>
      {#each PRESETS as p (p)}
        <option value={String(p)}>{Math.round(p * 100)}%</option>
      {/each}
      {#if choice === "custom"}
        <option value="custom">{Math.round(zoom * 100)}%</option>
      {/if}
    </select>
    <button type="button" onclick={() => onZoomStep(1)} title="Ctrl+Shift++">拡大</button>
  </div>
  <div class="group page">
    <input
      bind:this={pageInput}
      bind:value={pageText}
      onkeydown={onPageKey}
      onfocus={() => pageInput?.select()}
      onblur={() => (pageText = String(currentPage + 1))}
      inputmode="numeric"
      aria-label="ページ番号"
      title="Ctrl+G"
    />
    <span>/ {pageCount}</span>
  </div>
  <div class="spacer"></div>
  {#if hasOutline}
    <button type="button" onclick={onOutline} title="Ctrl+B">しおり</button>
  {/if}
  <button type="button" class:pressed={listVisible} aria-pressed={listVisible} onclick={onToggleList} title="F4"
    >コメント一覧</button
  >
</div>

<style>
  .toolbar {
    display: flex;
    align-items: center;
    gap: var(--space-3);
    height: var(--toolbar-h);
    padding: 0 var(--space-2);
    border-bottom: 1px solid var(--line);
    background: var(--bg);
  }

  .group {
    display: flex;
    align-items: center;
    gap: var(--space-1);
  }

  .spacer {
    flex: 1;
  }

  button,
  select,
  input {
    height: 24px;
    font: inherit;
    color: var(--fg);
    background: transparent;
    border: 1px solid transparent;
    border-radius: var(--radius);
  }

  button {
    padding: 0 var(--space-2);
  }

  button:hover,
  select:hover {
    background: var(--hover);
    border-color: var(--line);
  }

  button.pressed {
    background: var(--hover);
    border-color: var(--fg-muted);
  }

  select {
    padding: 0 2px;
    border-color: var(--line);
    background: var(--bg-panel);
  }

  input {
    width: 40px;
    padding: 0 4px;
    text-align: right;
    border-color: var(--line);
    background: var(--bg-panel);
    font-variant-numeric: tabular-nums;
  }

  .page span {
    color: var(--fg-muted);
    font-variant-numeric: tabular-nums;
  }
</style>
