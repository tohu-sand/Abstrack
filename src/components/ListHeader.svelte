<script lang="ts">
  import { tick } from "svelte";
  import type { CommentRow } from "../lib/backend";
  import { narrowChoices, type ListView } from "../lib/listView.svelte";

  interface Props {
    view: ListView;
    rows: CommentRow[] | null;
    total: number;
    shownCount: number;
  }

  let { view, rows, total, shownCount }: Props = $props();
  let search: HTMLInputElement | undefined = $state();
  let status: HTMLSelectElement | undefined = $state();
  let toggle: HTMLButtonElement | undefined = $state();
  let expanded = $state(false);
  let choices = $derived(narrowChoices(rows));
  // The author or kind narrowed to may no longer be among the rows; it keeps an
  // option so that the select still shows what the list is narrowed to.
  let lost = $derived(view.narrow !== "" &&
    !choices.authors.some((a) => view.narrow === `author:${a}`) &&
    !choices.kinds.some((k) => view.narrow === `kind:${k}`));

  export function focusSearch() {
    search?.focus();
    search?.select();
  }

  export async function focusFilter() {
    expanded = true;
    await tick();
    status?.focus();
  }

  function onKeydown(e: KeyboardEvent) {
    if (e.key !== "Escape" || e.isComposing) return;
    if (e.target === search && view.query) view.query = "";
    else if (expanded) {
      expanded = false;
      toggle?.focus();
    } else return;
    e.preventDefault();
    e.stopPropagation();
  }
</script>

<header>
  <div class="controls">
    <input bind:this={search} bind:value={view.query} onkeydown={onKeydown} type="search"
      placeholder="コメントを検索" aria-label="コメントを検索" title="Ctrl+Shift+F" />
    <button type="button" bind:this={toggle} aria-expanded={expanded} aria-controls="comment-filters"
      onclick={() => (expanded = !expanded)} onkeydown={onKeydown}>絞り込み</button>
  </div>
  <div class="filters" id="comment-filters" hidden={!expanded}>
    <select bind:this={status} bind:value={view.status} onkeydown={onKeydown} aria-label="対応状況">
      <option value="all">すべて</option>
      <option value="unchecked">未対応のみ</option>
      <option value="checked">対応済みのみ</option>
    </select>
    <select bind:value={view.narrow} onkeydown={onKeydown} aria-label="作成者・種類">
      <option value="">作成者・種類</option>
      {#if lost}<option value={view.narrow}>{view.narrow.slice(view.narrow.indexOf(":") + 1)}</option>{/if}
      <optgroup label="作成者">
        {#each choices.authors as author (author)}
          <option value="author:{author}">{author || "作成者なし"}</option>
        {/each}
      </optgroup>
      <optgroup label="種類">
        {#each choices.kinds as kind (kind)}
          <option value="kind:{kind}">{kind}</option>
        {/each}
      </optgroup>
    </select>
    <select bind:value={view.order} onkeydown={onKeydown} aria-label="並べ替え">
      <option value="reading">ページ順</option>
      <option value="date">日付順</option>
      <option value="author">作成者順</option>
    </select>
  </div>
  {#if view.filtering}
    <div class="result">
      <span role="status">{total} 件中 {shownCount} 件を表示</span>
      <button type="button" onclick={() => { view.clear(); search?.focus(); }}>解除</button>
    </div>
  {/if}
</header>

<style>
  header {
    padding: var(--space-2);
    border-bottom: 1px solid var(--line);
    font-size: var(--font-size-ui);
  }

  .controls, .filters:not([hidden]), .result {
    display: flex;
    align-items: center;
    gap: var(--space-2);
  }

  .filters, .result {
    margin-top: var(--space-1);
    flex-wrap: wrap;
  }

  input, button, select {
    height: 24px;
    min-width: 0;
    border: 1px solid var(--line);
    border-radius: var(--radius);
    background: var(--bg);
    color: var(--fg);
    font: inherit;
  }

  input {
    flex: 1;
    padding: 0 var(--space-2);
  }

  button {
    padding: 0 var(--space-2);
    background: transparent;
  }

  button:hover, button[aria-expanded="true"] {
    background: var(--hover);
  }

  select {
    max-width: 100%;
    flex: 1 1 6em;
  }

  .result {
    justify-content: space-between;
    color: var(--fg-muted);
  }
</style>
