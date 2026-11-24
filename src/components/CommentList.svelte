<script lang="ts">
  import { tick } from "svelte";
  import type { CommentRow } from "../lib/backend";
  import type { Editing } from "../lib/commenting.svelte";
  import { pageGroups, type ListView } from "../lib/listView.svelte";
  import CommentRowView from "./CommentRowView.svelte";
  import ListHeader from "./ListHeader.svelte";

  interface Props {
    rows: CommentRow[] | null;
    /** The rows the search and filters let through, in the list's order. */
    shown: CommentRow[];
    view: ListView;
    /** Each row's number, by key. */
    numbers: Map<string, number>;
    total: number;
    fontSize: number;
    selectedKey: string | null;
    /** The row whose text is being typed, if any. */
    editing: Editing | null;
    editable: boolean;
    onSelect: (key: string) => void;
    /** A click in the list away from every row. */
    onDeselect: () => void;
    /** A right click on a row. */
    onMenu: (key: string) => void;
    onEdit: (key: string) => void;
    onReply: (key: string) => void;
    onDelete: (key: string) => void;
    onToggle: (key: string) => void;
    onCopy: (row: CommentRow) => void;
    /** The text typed into the field opened for `field`. */
    onCommit: (field: Editing, text: string) => void;
    onCancel: (field: Editing) => void;
  }

  let {
    rows,
    shown,
    view,
    numbers,
    total,
    fontSize,
    selectedKey,
    editing,
    editable,
    onSelect,
    onDeselect,
    onMenu,
    onEdit,
    onReply,
    onDelete,
    onToggle,
    onCopy,
    onCommit,
    onCancel,
  }: Props = $props();

  let listEl: HTMLDivElement | undefined = $state();
  let header: ListHeader | undefined = $state();
  let groups = $derived(view.byPage ? pageGroups(shown) : null);
  let selectedVisible = $derived(selectedKey !== null && shown.some((r) => r.key === selectedKey));

  export function focusSearch(): void {
    header?.focusSearch();
  }

  export function focusFilter(): void {
    header?.focusFilter();
  }

  /** Whether the keyboard is on the rows (not on a field of the list). */
  export function hasFocus(): boolean {
    return !!listEl && listEl.contains(document.activeElement);
  }

  $effect(() => {
    const key = selectedKey;
    if (!key || !listEl) return;
    const el = listEl.querySelector<HTMLElement>(`[data-key="${CSS.escape(key)}"]`);
    el?.scrollIntoView({ block: "nearest" });
  });

  async function move(index: number) {
    if (shown.length === 0) return;
    const key = shown[Math.min(shown.length - 1, Math.max(0, index))]!.key;
    onSelect(key);
    await tick();
    listEl?.querySelector<HTMLElement>(`[data-key="${CSS.escape(key)}"]`)?.focus({ preventScroll: true });
  }

  function onKeydown(event: KeyboardEvent) {
    if (event.defaultPrevented || event.target instanceof HTMLTextAreaElement || event.target instanceof HTMLButtonElement) return;
    const index = shown.findIndex((r) => r.key === selectedKey);
    const keys: Record<string, () => void> = {
      ArrowDown: () => move(index + 1),
      ArrowUp: () => move(index < 0 ? shown.length - 1 : index - 1),
      Home: () => move(0),
      End: () => move(shown.length - 1),
      Enter: () => selectedVisible && selectedKey && editable && onEdit(selectedKey),
      Delete: () => selectedVisible && selectedKey && editable && onDelete(selectedKey),
    };
    const action = keys[event.key];
    if (action && !event.ctrlKey && !event.altKey) {
      event.preventDefault();
      action();
    }
  }
</script>

<section class="list" style:font-size="{fontSize}px" aria-label="コメント一覧">
  <ListHeader bind:this={header} {view} {rows} {total} shownCount={shown.length} />
  {#if rows === null}
    <p class="placeholder">コメントを読み込んでいます…</p>
  {:else if rows.length === 0}
    <p class="placeholder">この文書にはコメントがありません</p>
  {:else if shown.length === 0}
    <p class="placeholder">条件に合うコメントはありません</p>
  {:else}
    <div
      class="rows"
      class:by-page={groups !== null}
      bind:this={listEl}
      tabindex={selectedVisible ? -1 : 0}
      role="grid"
      aria-label="コメント"
      onkeydown={onKeydown}
      onclick={(e) => {
        if (e.target instanceof Element && !e.target.closest("[data-key]")) onDeselect();
      }}
    >
      {#if groups}
        {#each groups as group (group.page)}
          <div role="rowgroup" aria-label="ページ {group.page + 1}">
            <div class="page" aria-hidden="true">ページ {group.page + 1}</div>
            {#each group.rows as row (row.key)}
              {@render rowView(row)}
            {/each}
          </div>
        {/each}
      {:else}
        {#each shown as row (row.key)}
          {@render rowView(row)}
        {/each}
      {/if}
    </div>
  {/if}
</section>

{#snippet rowView(row: CommentRow)}
  <div role="row">
    <CommentRowView
      {row}
      number={numbers.get(row.key)}
      selected={row.key === selectedKey}
      byPage={groups !== null}
      {editing}
      {editable}
      {onSelect}
      {onMenu}
      {onToggle}
      {onEdit}
      {onReply}
      {onCopy}
      {onCommit}
      {onCancel}
      {onKeydown}
    />
  </div>
{/snippet}

<style>
  .list {
    display: grid;
    grid-template-columns: minmax(0, 1fr);
    grid-template-rows: auto 1fr;
    min-height: 0;
    background: var(--bg-panel);
    border-left: 1px solid var(--line);
  }

  .placeholder {
    margin: var(--space-3) var(--space-2);
    color: var(--fg-muted);
    font-size: var(--font-size-ui);
  }

  .rows {
    overflow-y: auto;
    scrollbar-gutter: stable;
  }

  .rows:focus-visible {
    outline-offset: -2px;
  }

  /* A row brought into view is not left hidden under the page heading. */
  .rows.by-page {
    scroll-padding-top: 24px;
  }

  /* Stays at the top while its page's rows scroll under it, since the rows
     do not show their page. */
  .page {
    position: sticky;
    top: 0;
    z-index: 1;
    display: flex;
    align-items: center;
    height: 24px;
    padding: 0 var(--space-3);
    border-bottom: 1px solid var(--line);
    background: var(--bg);
    color: var(--fg-muted);
    font-size: var(--font-size-ui);
    font-weight: 600;
  }
</style>
