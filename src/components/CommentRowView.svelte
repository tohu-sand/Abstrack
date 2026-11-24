<script lang="ts">
  import type { CommentRow } from "../lib/backend";
  import { editableText, prompt, type Editing } from "../lib/commenting.svelte";
  import CommentEditor from "./CommentEditor.svelte";

  interface Props {
    row: CommentRow;
    number: number | undefined;
    selected: boolean;
    /** The list names the page above its rows, so the row does not. */
    byPage: boolean;
    editing: Editing | null;
    editable: boolean;
    onSelect: (key: string) => void;
    /** A right click on the row. */
    onMenu: (key: string) => void;
    onToggle: (key: string) => void;
    onEdit: (key: string) => void;
    onReply: (key: string) => void;
    onCopy: (row: CommentRow) => void;
    onCommit: (field: Editing, text: string) => void;
    onCancel: (field: Editing) => void;
    onKeydown: (event: KeyboardEvent) => void;
  }

  let {
    row,
    number,
    selected,
    byPage,
    editing,
    editable,
    onSelect,
    onMenu,
    onToggle,
    onEdit,
    onReply,
    onCopy,
    onCommit,
    onCancel,
    onKeydown,
  }: Props = $props();

  // Acrobat ends lines with a carriage return alone, which pre-wrap shows as a space.
  function lines(text: string): string {
    return text.replace(/\r\n?/g, "\n");
  }

  let field = $derived(editing?.key === row.key ? editing.mode : null);
  let details = $derived([row.kind, row.author, row.date, row.review].filter(Boolean).join(" · "));
</script>

<div
  class="row"
  class:selected
  class:marked={row.marked}
  data-key={row.key}
  role="gridcell"
  aria-selected={selected}
  tabindex={selected ? 0 : -1}
  onclick={() => onSelect(row.key)}
  ondblclick={(e) => {
    if (editable && e.target instanceof Element && !e.target.closest("button, textarea")) onEdit(row.key);
  }}
  oncontextmenu={(e) => {
    if (e.target instanceof HTMLTextAreaElement) return;
    e.preventDefault();
    onMenu(row.key);
  }}
  onkeydown={onKeydown}
>
  <button type="button" class="check" role="checkbox" aria-checked={row.marked}
    aria-label={`#${number} ${row.marked ? "チェックを外す" : "チェックを付ける"}`}
    title={row.marked ? "チェックを外す" : "チェックを付ける"}
    disabled={!editable} tabindex={selected ? 0 : -1}
    onmousedown={(e) => e.preventDefault()}
    onclick={(e) => { e.stopPropagation(); onToggle(row.key); }}>
    <span aria-hidden="true">{row.marked ? "✓" : ""}</span>
  </button>
  <span class="number comment-number" style:--annotation-color={row.color} title={details}>#{number}</span>
  <div class="content">
    {#if !byPage}<span class="page">p{row.page + 1}</span>{/if}
    {#if field === "body"}
      <CommentEditor initial={editableText(row)} placeholder={prompt(row, "body")}
        onCommit={(text) => onCommit({ key: row.key, mode: "body" }, text)}
        onCancel={() => onCancel({ key: row.key, mode: "body" })} />
    {:else}
      {#if row.editText}
        <button type="button" class="edit-text" title="クリックでコピー" aria-label={`「${row.editText}」をコピー`}
          tabindex={selected ? 0 : -1} onmousedown={(e) => e.preventDefault()}
          onclick={(e) => { e.stopPropagation(); onCopy(row); }}>{row.editText}</button>
      {/if}
      {#if row.body}
        <div class="body">{lines(row.body)}</div>
      {:else if !row.editText && row.target}
        <div class="target">「{row.target}」</div>
      {/if}
    {/if}
    {#if selected}
      {#each row.replies as reply, i (i)}
        <div class="reply" title={[reply.author, reply.date].filter(Boolean).join(" · ")}>{lines(reply.text)}</div>
      {/each}
    {:else if row.replies.length}
      <span class="reply-count">{row.replies.length} 件の返信</span>
    {/if}
    {#if field === "reply"}
      <CommentEditor initial="" placeholder={prompt(row, "reply")}
        onCommit={(text) => onCommit({ key: row.key, mode: "reply" }, text)}
        onCancel={() => onCancel({ key: row.key, mode: "reply" })} />
    {:else if selected && editable && !field}
      <button type="button" class="reply-action" onclick={(e) => { e.stopPropagation(); onReply(row.key); }}>返信</button>
    {/if}
  </div>
</div>

<style>
  .row {
    display: grid;
    grid-template-columns: 18px auto minmax(0, 1fr);
    align-items: start;
    column-gap: var(--space-1);
    padding: var(--space-2);
    min-height: 36px;
    border-bottom: 1px solid var(--line);
    content-visibility: auto;
    contain-intrinsic-size: auto 38px;
  }

  .row:hover, .row.selected {
    background: var(--hover);
  }

  .row.selected .number {
    outline: 1px solid currentColor;
  }

  .check {
    display: grid;
    place-items: center;
    width: 18px;
    height: 20px;
    padding: 0;
    border: 0;
    background: transparent;
    color: var(--fg-muted);
  }

  .check span {
    display: grid;
    place-items: center;
    width: 13px;
    height: 13px;
    border: 1px solid currentColor;
    border-radius: var(--radius);
    font: bold 11px/1 var(--font-ui);
  }

  .check:hover:enabled {
    color: var(--fg);
  }

  .number {
    margin-top: 2px;
  }

  .content {
    padding-left: var(--space-1);
    line-height: 1.5;
    overflow-wrap: anywhere;
  }

  .marked:not(.selected) .content, .target, .page, .reply-count {
    color: var(--fg-muted);
  }

  .body, .reply, .edit-text {
    white-space: pre-wrap;
  }

  .row:not(.selected) .target {
    display: -webkit-box;
    -webkit-box-orient: vertical;
    -webkit-line-clamp: 2;
    line-clamp: 2;
    overflow: hidden;
  }

  .edit-text, .reply-action {
    max-width: 100%;
    padding: 0;
    border: 0;
    background: transparent;
    color: inherit;
    font: inherit;
    text-align: left;
    overflow-wrap: anywhere;
  }

  .edit-text {
    cursor: copy;
    text-decoration: underline dotted var(--fg-muted);
    text-underline-offset: 3px;
  }

  .edit-text:hover, .reply-action:hover {
    text-decoration: underline;
  }

  .edit-text + .body {
    margin-top: var(--space-1);
  }

  .reply {
    margin-top: var(--space-1);
    padding-left: var(--space-2);
    border-left: 1px solid var(--fg-muted);
  }

  .page, .reply-count, .reply-action {
    color: var(--fg-muted);
    font-size: var(--font-size-ui);
  }

  .reply-action {
    margin-top: var(--space-1);
  }
</style>
