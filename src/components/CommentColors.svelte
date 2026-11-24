<script lang="ts">
  import { onMount } from "svelte";
  import { commentColors, setCommentColors, type KindColor } from "../lib/backend";
  import { KIND_LABELS, type CommentKind } from "../lib/selection";
  import Popover from "./Popover.svelte";

  interface Props {
    onClose: () => void;
    /** Reports what changed. */
    onDone: (message: string) => void;
    onError: (message: string) => void;
  }

  let { onClose, onDone, onError }: Props = $props();

  let colors = $state<KindColor[]>([]);

  onMount(() => void load());

  async function load() {
    try {
      colors = await commentColors();
    } catch (e) {
      onError(`コメントの色を読めませんでした: ${String(e)}`);
    }
  }

  /** `chosen`: the kinds not at their defaults. */
  async function save(chosen: Partial<Record<CommentKind, string>>, message: string) {
    try {
      await setCommentColors(chosen);
      onDone(message);
    } catch (e) {
      onError(`コメントの色を保存できませんでした: ${String(e)}`);
    }
    await load();
  }

  function choose(kind: CommentKind, color: string) {
    const chosen = Object.fromEntries(colors.filter((c) => c.changed).map((c) => [c.kind, c.color]));
    void save({ ...chosen, [kind]: color }, `${KIND_LABELS[kind][0]}の色を変えました`);
  }
</script>

<!-- A swatch opens the web view's color picker; a new color is saved when
     the picker closes. -->
<Popover title="コメントの色" {onClose}>
  <div class="colors">
    <ul>
      {#each colors as c (c.kind)}
        <li>
          <label>
            <input type="color" value={c.color} onchange={(e) => choose(c.kind, e.currentTarget.value)} />
            {KIND_LABELS[c.kind][0]}
          </label>
        </li>
      {/each}
    </ul>
    <button type="button" disabled={!colors.some((c) => c.changed)} onclick={() => save({}, "コメントの色を既定に戻しました")}
      >既定の色に戻す</button
    >
  </div>
</Popover>

<style>
  .colors {
    display: grid;
    gap: var(--space-2);
    padding: var(--space-2);
  }

  ul {
    display: grid;
    gap: var(--space-1);
    margin: 0;
    padding: 0;
    list-style: none;
  }

  label {
    display: flex;
    align-items: center;
    gap: var(--space-2);
  }

  input {
    width: 32px;
    height: 20px;
    padding: 1px;
    border: 1px solid var(--line);
    border-radius: var(--radius);
    background: var(--bg);
  }

  input:focus-visible {
    outline: none;
    border-color: var(--accent);
  }

  input::-webkit-color-swatch-wrapper {
    padding: 0;
  }

  input::-webkit-color-swatch {
    border: none;
    border-radius: 1px;
  }

  button {
    justify-self: start;
    height: 24px;
    padding: 0 var(--space-3);
    border: 1px solid var(--fg-muted);
    border-radius: var(--radius);
    background: var(--bg-panel);
    color: var(--fg);
    font: inherit;
  }

  button:hover:not(:disabled) {
    background: var(--hover);
  }

  button:disabled {
    border-color: var(--line);
    color: var(--fg-muted);
  }
</style>
