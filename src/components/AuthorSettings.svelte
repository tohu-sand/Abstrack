<script lang="ts">
  import { onMount } from "svelte";
  import { authorName, setAuthorName } from "../lib/backend";
  import Popover from "./Popover.svelte";

  interface Props {
    onClose: () => void;
    /** Reports the name now in use. */
    onDone: (message: string) => void;
    onError: (message: string) => void;
  }

  let { onClose, onDone, onError }: Props = $props();

  let name = $state("");
  let login = $state("");
  let field: HTMLInputElement | undefined = $state();

  onMount(() => {
    authorName()
      .then((a) => {
        name = a.name;
        login = a.login;
        queueMicrotask(() => field?.select());
      })
      .catch((e) => onError(`作成者名を読めませんでした: ${String(e)}`));
  });

  async function apply() {
    const chosen = name.trim();
    try {
      await setAuthorName(chosen);
      onDone(`作成者名を「${chosen || login}」にしました`);
      onClose();
    } catch (e) {
      onError(`作成者名を保存できませんでした: ${String(e)}`);
    }
  }

  function onKey(e: KeyboardEvent) {
    if (e.key === "Enter" && !e.isComposing) {
      e.preventDefault();
      void apply();
    }
  }
</script>

<Popover title="コメントの作成者名" {onClose}>
  <div class="author">
    <div class="row">
      <input bind:this={field} bind:value={name} placeholder={login} aria-label="作成者名" onkeydown={onKey} />
      <button type="button" onclick={apply}>設定</button>
    </div>
    <p>対応済みとして数えるのは、この名前で付けたチェックだけです。</p>
  </div>
</Popover>

<style>
  .author {
    display: grid;
    gap: var(--space-2);
    padding: var(--space-2);
  }

  .row {
    display: flex;
    gap: var(--space-1);
  }

  input {
    flex: 1;
    min-width: 0;
    height: 24px;
    padding: 0 var(--space-1);
    border: 1px solid var(--line);
    border-radius: var(--radius);
    background: var(--bg);
    color: var(--fg);
    font: inherit;
  }

  input:focus {
    outline: none;
    border-color: var(--accent);
  }

  button {
    height: 24px;
    padding: 0 var(--space-3);
    border: 1px solid var(--fg-muted);
    border-radius: var(--radius);
    background: var(--bg-panel);
    color: var(--fg);
    font: inherit;
  }

  button:hover {
    background: var(--hover);
  }

  p {
    margin: 0;
    color: var(--fg-muted);
  }
</style>
