<script lang="ts">
  import Popover from "./Popover.svelte";

  interface Props {
    onClose: () => void;
  }

  let { onClose }: Props = $props();

  const groups: { title: string; items: [string, string][] }[] = [
    {
      title: "ファイル",
      items: [
        ["Ctrl+O", "開く"],
        ["Ctrl+S", "上書き保存"],
        ["Ctrl+Shift+S", "名前を付けて保存"],
        ["Ctrl+W", "閉じる"],
      ],
    },
    {
      title: "コメントを入れる",
      items: [
        ["H", "ハイライト"],
        ["R", "置換"],
        ["D", "取り消し線"],
        ["U", "下線"],
        ["I", "挿入"],
        ["N", "メモ"],
        ["Alt+ドラッグ → Enter", "長方形"],
        ["右クリック", "その場所で使える操作を表示"],
      ],
    },
    {
      title: "コメントの編集",
      items: [
        ["Enter", "入力を確定 / 選んだコメントを編集"],
        ["Shift+Enter", "入力中の改行"],
        ["Esc", "入力をやめる / コメントの選択を解除"],
        ["Delete", "選んだコメントを削除"],
        ["ハンドルをドラッグ", "選んだコメントの範囲を変える"],
        ["Ctrl+Z / Ctrl+Shift+Z", "元に戻す / やり直し"],
      ],
    },
    {
      title: "表示",
      items: [
        ["F4", "コメント一覧の表示・非表示"],
        ["Ctrl+2", "幅に合わせる"],
        ["Ctrl+0", "ページ全体"],
        ["Ctrl+1", "実際のサイズ"],
        ["Ctrl+Shift++ / Ctrl+Shift+-", "拡大・縮小"],
        ["Ctrl+ホイール", "拡大・縮小"],
        ["Ctrl++ / Ctrl+-", "一覧の文字を大きく・小さく"],
      ],
    },
    {
      title: "選択",
      items: [
        ["ドラッグ", "文字を選択"],
        ["ダブルクリック / トリプルクリック", "単語 / 文を選択"],
        ["Shift+クリック", "選択範囲の終わりを変える"],
        ["Shift+← / Shift+→", "選択範囲を1文字ずつ広げる・狭める"],
        ["Ctrl+Shift+← / Ctrl+Shift+→", "選択範囲を1単語ずつ広げる・狭める"],
        ["Shift+↑ / Shift+↓", "選択範囲を1行ずつ広げる・狭める"],
        ["Ctrl+ドラッグ", "選択範囲を追加"],
        ["Alt+ドラッグ", "長方形で囲む"],
        ["Ctrl+F → 入力 → Esc", "見つけた文字を選択"],
        ["Esc", "選択を解除"],
      ],
    },
    {
      title: "編集",
      items: [
        ["Ctrl+C", "選択した文字をコピー"],
        ["Ctrl+F", "検索"],
        ["F3 / Shift+F3", "次・前を検索"],
      ],
    },
    {
      title: "移動",
      items: [
        ["Ctrl+PageDown / Ctrl+PageUp", "次のページ・前のページ"],
        ["Ctrl+Home / Ctrl+End", "最初のページ・最後のページ"],
        ["Ctrl+G", "ページを指定"],
        ["Ctrl+B", "しおり"],
      ],
    },
    {
      title: "コメントへの対応",
      items: [
        ["J / F8", "次の未対応のコメントへ"],
        ["K / Shift+F8", "前の未対応のコメントへ"],
        ["Space", "選んだコメントにチェックを付ける・外す"],
      ],
    },
    {
      title: "コメント一覧",
      items: [
        ["↑ / ↓", "前・次のコメント"],
        ["Home / End", "最初・最後のコメント"],
        ["Ctrl+Shift+F", "コメントを検索"],
        ["Esc", "コメントの検索語を消す"],
        ["Ctrl+C", "置換後の文字列・挿入する文字列をコピー"],
      ],
    },
    {
      title: "ヘルプ",
      items: [["F1", "この一覧"]],
    },
  ];
</script>

<Popover title="ショートカット一覧" {onClose}>
  {#each groups as g (g.title)}
    <h3>{g.title}</h3>
    <table>
      <tbody>
        {#each g.items as [keys, action] (keys)}
          <tr><th>{keys}</th><td>{action}</td></tr>
        {/each}
      </tbody>
    </table>
  {/each}
</Popover>

<style>
  h3 {
    margin: var(--space-2) var(--space-2) 2px;
    font-size: var(--font-size-ui);
    font-weight: 600;
  }

  table {
    width: 100%;
    border-collapse: collapse;
    table-layout: fixed;
  }

  th,
  td {
    padding: 2px var(--space-2);
    border-bottom: 1px solid var(--line);
    text-align: left;
    vertical-align: top;
    font-weight: normal;
  }

  th {
    width: 45%;
    white-space: nowrap;
    font-variant-numeric: tabular-nums;
  }
</style>
