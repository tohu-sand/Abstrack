<script lang="ts">
  import type { Notice } from "../lib/notices.svelte";
  import { titleIfCut } from "../lib/tooltip";

  interface Props {
    message: string;
    error: string | null;
    /** Stays until answered with one of its buttons. */
    notice: Notice | null;
    counts: { unchecked: number; total: number } | null;
    /** What is selected on the page, already shortened. */
    selection: string;
    /** Whether there are changes to save; `null` when nothing can be saved. */
    dirty: boolean | null;
    onDismissError: () => void;
    onSave: () => void;
  }

  let { message, error, notice, counts, selection, dirty, onDismissError, onSave }: Props = $props();
</script>

<footer class="statusbar">
  {#if error}
    <span class="error" {@attach titleIfCut(error)}>
      {error}
      <button type="button" onclick={onDismissError} title="閉じる" aria-label="エラー表示を閉じる">×</button>
    </span>
  {/if}
  {#if notice}
    <span class="notice">
      <span class="text" {@attach titleIfCut(notice.text)}>{notice.text}</span>
      {#each notice.actions as action (action.label)}
        <button type="button" onclick={action.run} title={action.title}>{action.label}</button>
      {/each}
    </span>
  {/if}
  {#if selection}
    <span class="selection" {@attach titleIfCut(selection)}>{selection}</span>
  {/if}
  <span class="message" {@attach titleIfCut(message)}>{message}</span>
  {#if counts}
    <span class="counts">未対応 {counts.unchecked} / 全 {counts.total}</span>
  {/if}
  {#if dirty === true}
    <button type="button" class="save" onclick={onSave} title="上書き保存 (Ctrl+S)">未保存</button>
  {:else if dirty === false}
    <span class="saved">保存済み</span>
  {/if}
</footer>

<style>
  .statusbar {
    display: flex;
    align-items: center;
    gap: var(--space-3);
    height: var(--statusbar-h);
    padding: 0 var(--space-2);
    border-top: 1px solid var(--line);
    background: var(--bg);
    white-space: nowrap;
    overflow: hidden;
  }

  .message {
    flex: 1;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .error {
    display: flex;
    align-items: center;
    gap: var(--space-1);
    max-width: 60%;
    overflow: hidden;
    text-overflow: ellipsis;
    color: var(--error);
  }

  .error button {
    height: 18px;
    padding: 0 4px;
    border: 1px solid transparent;
    border-radius: var(--radius);
    background: transparent;
    color: inherit;
    font: inherit;
  }

  .error button:hover {
    border-color: var(--line);
  }

  .notice {
    display: flex;
    align-items: center;
    gap: var(--space-1);
    max-width: 70%;
    min-width: 0;
    font-weight: 600;
  }

  .notice .text {
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .notice button {
    flex: none;
    height: 18px;
    padding: 0 var(--space-2);
    border: 1px solid var(--fg-muted);
    border-radius: var(--radius);
    background: var(--bg-panel);
    color: var(--fg);
    font: inherit;
    font-weight: normal;
  }

  .notice button:hover {
    background: var(--hover);
  }

  .selection {
    max-width: 40%;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .counts {
    font-variant-numeric: tabular-nums;
  }

  .saved,
  .save {
    padding-left: var(--space-2);
    border-left: 1px solid var(--line);
  }

  .save {
    height: 18px;
    border-top: 0;
    border-right: 0;
    border-bottom: 0;
    background: transparent;
    color: var(--fg);
    font: inherit;
    font-weight: 600;
  }

  .save:hover {
    text-decoration: underline;
  }
</style>
