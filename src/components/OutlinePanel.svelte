<script lang="ts">
  import type { OutlineItem } from "../lib/backend";
  import Popover from "./Popover.svelte";

  interface Props {
    items: OutlineItem[];
    onGo: (page: number) => void;
    onClose: () => void;
  }

  let { items, onGo, onClose }: Props = $props();
</script>

<Popover title="しおり" {onClose}>
  <ul>
    {#each items as item, i (i)}
      <li>
        <button
          type="button"
          style:padding-left="{8 + (item.level - 1) * 14}px"
          onclick={() => {
            onGo(item.page);
            onClose();
          }}
        >
          <span class="title">{item.title}</span>
          <span class="page">{item.page + 1}</span>
        </button>
      </li>
    {/each}
  </ul>
</Popover>

<style>
  ul {
    margin: 0;
    padding: 0;
    list-style: none;
  }

  li {
    border-bottom: 1px solid var(--line);
  }

  button {
    display: flex;
    gap: var(--space-2);
    width: 100%;
    padding: 4px 8px;
    border: none;
    background: transparent;
    font: inherit;
    color: var(--fg);
    text-align: left;
  }

  button:hover,
  button:focus-visible {
    background: var(--hover);
  }

  .title {
    flex: 1;
    min-width: 0;
    overflow-wrap: anywhere;
  }

  .page {
    color: var(--fg-muted);
    font-variant-numeric: tabular-nums;
  }
</style>
