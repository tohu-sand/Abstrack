<script lang="ts">
  import { titleIfCut } from "../lib/tooltip";

  interface Props {
    recent: string[];
    onOpen: (path: string) => void;
  }

  let { recent, onOpen }: Props = $props();

  function split(path: string): { name: string; dir: string } {
    const i = Math.max(path.lastIndexOf("\\"), path.lastIndexOf("/"));
    return { name: path.slice(i + 1), dir: path.slice(0, Math.max(0, i)) };
  }
</script>

<div class="start">
  <p class="hint">PDFをドロップ、または Ctrl+O</p>
  {#if recent.length > 0}
    <h2>最近使ったファイル</h2>
    <ul>
      {#each recent as path (path)}
        {@const p = split(path)}
        <li>
          <button type="button" onclick={() => onOpen(path)}>
            <span class="name">{p.name}</span>
            <span class="dir" {@attach titleIfCut(p.dir)}>{p.dir}</span>
          </button>
        </li>
      {/each}
    </ul>
  {/if}
</div>

<style>
  .start {
    padding: var(--space-4);
    overflow: auto;
    background: var(--bg-canvas);
  }

  .hint {
    margin: 0 0 var(--space-4);
  }

  h2 {
    margin: 0 0 var(--space-1);
    font-size: var(--font-size-ui);
    font-weight: normal;
    color: var(--fg-muted);
  }

  ul {
    margin: 0;
    padding: 0;
    list-style: none;
    max-width: 720px;
    border-top: 1px solid var(--line);
  }

  li {
    border-bottom: 1px solid var(--line);
  }

  button {
    display: flex;
    gap: var(--space-3);
    width: 100%;
    padding: 6px var(--space-2);
    border: none;
    background: transparent;
    font: inherit;
    color: var(--fg);
    text-align: left;
  }

  button:hover {
    background: var(--hover);
  }

  .name {
    flex: none;
  }

  .dir {
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    color: var(--fg-muted);
  }
</style>
