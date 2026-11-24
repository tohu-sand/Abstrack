<script lang="ts">
  import type { Snippet } from "svelte";

  interface Props {
    title: string;
    onClose: () => void;
    children: Snippet;
  }

  let { title, onClose, children }: Props = $props();
  let panel: HTMLElement | undefined = $state();

  $effect(() => {
    panel?.focus();
    const onKey = (event: KeyboardEvent) => {
      if (event.key === "Escape") onClose();
    };
    const onPointer = (event: PointerEvent) => {
      if (panel && !panel.contains(event.target as Node)) onClose();
    };
    window.addEventListener("keydown", onKey);
    window.addEventListener("pointerdown", onPointer, true);
    return () => {
      window.removeEventListener("keydown", onKey);
      window.removeEventListener("pointerdown", onPointer, true);
    };
  });
</script>

<section class="popover" bind:this={panel} tabindex="-1" aria-label={title}>
  <header>
    <span>{title}</span>
    <button type="button" onclick={onClose} title="閉じる (Esc)" aria-label="閉じる">×</button>
  </header>
  <div class="body">
    {@render children()}
  </div>
</section>

<style>
  .popover {
    position: absolute;
    top: calc(var(--toolbar-h) + 4px);
    right: var(--space-2);
    z-index: 10;
    display: grid;
    grid-template-rows: auto 1fr;
    width: 380px;
    max-height: 70vh;
    background: var(--bg-panel);
    border: 1px solid var(--line);
    border-radius: var(--radius);
    box-shadow: 0 4px 16px rgb(0 0 0 / 18%);
    outline: none;
  }

  header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: var(--space-1) var(--space-2);
    border-bottom: 1px solid var(--line);
    color: var(--fg-muted);
  }

  header button {
    height: 20px;
    padding: 0 6px;
    border: 1px solid transparent;
    border-radius: var(--radius);
    background: transparent;
    font: inherit;
    color: var(--fg);
  }

  header button:hover {
    border-color: var(--line);
  }

  .body {
    overflow: auto;
    min-height: 0;
  }
</style>
