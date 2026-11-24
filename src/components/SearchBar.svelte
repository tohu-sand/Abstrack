<script lang="ts">
  interface Props {
    query: string;
    current: number;
    total: number | null;
    busy: boolean;
    onQuery: (query: string) => void;
    onStep: (direction: 1 | -1) => void;
    onClose: () => void;
  }

  let { query, current, total, busy, onQuery, onStep, onClose }: Props = $props();
  let input: HTMLInputElement | undefined = $state();

  $effect(() => {
    input?.focus();
    input?.select();
  });

  function onKeydown(event: KeyboardEvent) {
    if (event.isComposing) return;
    if (event.key === "Enter") {
      event.preventDefault();
      onStep(event.shiftKey ? -1 : 1);
    } else if (event.key === "Escape") {
      event.preventDefault();
      onClose();
    }
  }

  export function focus() {
    input?.focus();
    input?.select();
  }
</script>

<div class="search" role="search">
  <input
    bind:this={input}
    value={query}
    oninput={(event) => { if (!(event instanceof InputEvent && event.isComposing)) onQuery(event.currentTarget.value); }}
    oncompositionend={(event) => onQuery(event.currentTarget.value)}
    onkeydown={onKeydown}
    placeholder="テキストを検索"
    aria-label="テキストを検索 (Ctrl+F)"
  />
  <span class="count">
    {#if busy}検索中…{:else if total === null}{""}{:else if total === 0}見つかりません{:else}{current + 1} / {total} 件{/if}
  </span>
  <button type="button" onclick={() => onStep(-1)} disabled={!total} title="Shift+Enter, Shift+F3">前へ</button>
  <button type="button" onclick={() => onStep(1)} disabled={!total} title="Enter, F3">次へ</button>
  <button type="button" onclick={onClose} title="閉じる (Esc)" aria-label="検索を閉じる">×</button>
</div>

<style>
  .search {
    display: flex;
    align-items: center;
    gap: var(--space-1);
    height: 30px;
    padding: 0 var(--space-2);
    border-bottom: 1px solid var(--line);
    background: var(--bg);
  }

  input {
    width: 260px;
    height: 24px;
    padding: 0 6px;
    font: inherit;
    color: var(--fg);
    background: var(--bg-panel);
    border: 1px solid var(--line);
    border-radius: var(--radius);
  }

  .count {
    min-width: 96px;
    color: var(--fg-muted);
    font-variant-numeric: tabular-nums;
  }

  button {
    height: 24px;
    padding: 0 var(--space-2);
    font: inherit;
    color: var(--fg);
    background: transparent;
    border: 1px solid transparent;
    border-radius: var(--radius);
  }

  button:hover:not(:disabled) {
    background: var(--hover);
    border-color: var(--line);
  }

  button:disabled {
    color: var(--fg-muted);
  }
</style>
