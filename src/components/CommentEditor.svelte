<script lang="ts">
  import { tick } from "svelte";

  interface Props {
    initial: string;
    placeholder: string;
    /** Enter, or leaving the field: the text typed is kept. */
    onCommit: (text: string) => void;
    /** Esc. */
    onCancel: () => void;
  }

  let { initial, placeholder, onCommit, onCancel }: Props = $props();

  let field: HTMLTextAreaElement | undefined = $state();
  let done = false;

  $effect(() => {
    if (!field) return;
    field.value = initial;
    fit(field);
    field.focus();
    field.setSelectionRange(field.value.length, field.value.length);
  });

  function fit(el: HTMLTextAreaElement) {
    el.style.height = "auto";
    el.style.height = `${el.scrollHeight + 2}px`;
  }

  function finish(commit: boolean) {
    if (done || !field) return;
    done = true;
    if (commit) onCommit(field.value);
    else onCancel();
  }

  async function onKeydown(e: KeyboardEvent) {
    // Enter while an input method is converting confirms the conversion.
    if (e.isComposing || e.keyCode === 229) return;
    e.stopPropagation();
    const row = field?.closest<HTMLElement>("[data-key]");
    if (e.key === "Enter" && !e.shiftKey && !e.ctrlKey && !e.altKey) {
      e.preventDefault();
      finish(true);
    } else if (e.key === "Escape") {
      e.preventDefault();
      finish(false);
    } else return;
    await tick();
    if (document.activeElement === document.body) row?.focus({ preventScroll: true });
  }
</script>

<textarea
  bind:this={field}
  class="editor"
  rows="1"
  {placeholder}
  aria-label={placeholder}
  oninput={(e) => fit(e.currentTarget)}
  onkeydown={onKeydown}
  onblur={() => finish(true)}
  onclick={(e) => e.stopPropagation()}
></textarea>

<style>
  .editor {
    display: block;
    box-sizing: border-box;
    width: 100%;
    min-height: 2.2em;
    margin-top: var(--space-1);
    padding: 3px 5px;
    resize: none;
    border: 1px solid var(--accent);
    border-radius: var(--radius);
    background: var(--bg);
    color: var(--fg);
    font: inherit;
    line-height: 1.45;
    outline: none;
  }
</style>
