<script lang="ts">
  import { KIND_LABELS, type CommentKind } from "../lib/selection";

  interface Props {
    /** Position in the page area's content, CSS px. */
    left: number;
    top: number;
    kinds: CommentKind[];
    onPick: (kind: CommentKind) => void;
  }

  let { left, top, kinds, onPick }: Props = $props();
</script>

<!-- Pressing a button, or right-clicking it, must not change the selection
     on the page below. -->
<div
  class="actions"
  style:left="{left}px"
  style:top="{top}px"
  role="toolbar"
  aria-label="コメントを入れる"
  tabindex="-1"
  onmousedown={(e) => e.stopPropagation()}
  onpointerdown={(e) => e.stopPropagation()}
  oncontextmenu={(e) => {
    e.preventDefault();
    e.stopPropagation();
  }}
>
  {#each kinds as kind (kind)}
    {@const [label, key] = KIND_LABELS[kind]}
    <button type="button" onclick={() => onPick(kind)}>{label}({key})</button>
  {/each}
</div>

<style>
  .actions {
    position: absolute;
    z-index: 3;
    display: flex;
    gap: 1px;
    padding: 2px;
    background: var(--bg-panel);
    border: 1px solid var(--line);
    border-radius: var(--radius);
    box-shadow: 0 2px 8px rgb(0 0 0 / 16%);
    cursor: default;
    white-space: nowrap;
  }

  button {
    height: 22px;
    padding: 0 6px;
    border: 1px solid transparent;
    border-radius: var(--radius);
    background: transparent;
    color: var(--fg);
    font: inherit;
    font-size: var(--font-size-ui);
  }

  button:hover {
    border-color: var(--line);
    background: var(--hover);
  }

  button:focus-visible {
    outline: none;
    border-color: var(--accent);
  }
</style>
