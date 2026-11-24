<script lang="ts">
  import { onMount, tick } from "svelte";
  import * as api from "./lib/backend";
  import type { CommentRow, DocumentInfo, OutlineItem } from "./lib/backend";
  import { nextZoom } from "./lib/layout";
  import { watchIdle } from "./lib/idle";
  import { abbreviate, type Area, type Selection } from "./lib/selection";
  import { Checking } from "./lib/checking";
  import { Commenting } from "./lib/commenting.svelte";
  import { commentMenu, showMenu } from "./lib/contextMenus";
  import { recordDamage } from "./lib/damage";
  import { EditCommands } from "./lib/editCommands";
  import { keepUnchanged, numbersOf } from "./lib/rows";
  import { ListPanel } from "./lib/listPanel.svelte";
  import { ListView } from "./lib/listView.svelte";
  import { menuCommands } from "./lib/menuCommands";
  import { Notices } from "./lib/notices.svelte";
  import { Searching } from "./lib/searching.svelte";
  import { Zooming } from "./lib/zooming.svelte";
  import { windowKey } from "./lib/windowKeys";
  import AuthorSettings from "./components/AuthorSettings.svelte";
  import CommentColors from "./components/CommentColors.svelte";
  import CommentList from "./components/CommentList.svelte";
  import OutlinePanel from "./components/OutlinePanel.svelte";
  import Paper from "./components/Paper.svelte";
  import SearchBar from "./components/SearchBar.svelte";
  import ShortcutHelp from "./components/ShortcutHelp.svelte";
  import StartScreen from "./components/StartScreen.svelte";
  import StatusBar from "./components/StatusBar.svelte";
  import Toolbar from "./components/Toolbar.svelte";

  let doc = $state<DocumentInfo | null>(null);
  /** The application has not said yet whether a document is open. */
  let starting = $state(true);
  let rows = $state<CommentRow[] | null>(null);
  let outlineItems = $state<OutlineItem[]>([]);
  let recent = $state<string[]>([]);
  let currentPage = $state(0);
  let selectedKey = $state<string | null>(null);
  let flashKey = $state<string | null>(null);
  let message = $state("");
  let error = $state<string | null>(null);
  let panel = $state<"outline" | "help" | "author" | "colors" | null>(null);
  let paper: Paper | undefined = $state();
  let toolbar: Toolbar | undefined = $state();
  let searchBar: SearchBar | undefined = $state();
  let commentList: CommentList | undefined = $state();
  let selection = $state<Selection[]>([]);
  let area = $state<Area | null>(null);
  let selectionLabel = $derived.by(() => {
    if (selection.length === 1) return `選択: 「${abbreviate(selection[0]!.text)}」`;
    if (selection.length > 1) {
      return `選択 ${selection.length}か所: 「${abbreviate(selection.map((s) => s.text).join(" / "))}」`;
    }
    return area ? `選択: ${area.page + 1}ページの長方形` : "";
  });

  let counts = $derived(rows ? { unchecked: rows.filter((r) => !r.marked).length, total: rows.length } : null);
  let numbers = $derived(numbersOf(rows));
  const listView = new ListView();
  let shown = $derived(listView.shown(rows, numbers));
  const selectedRow = () => rows?.find((r) => r.key === selectedKey);
  /** Called when a page of the document being loaded comes on screen. */
  let pagePainted = () => {};

  function report(text: string) {
    error = text;
  }

  const list = new ListPanel(report);

  const notices = new Notices({ doc: () => doc, currentPage: () => currentPage, error: report });

  const view = new Zooming({
    doc: () => doc,
    currentPage: () => currentPage,
    anchorAt: (x, y) => paper?.anchorAt(x, y),
    restoreAnchor: (anchor, x, y) => paper?.restoreAnchor(anchor, x, y),
    scrollToPage: (page) => paper?.scrollToPage(page),
  });

  const search = new Searching({
    doc: () => doc,
    currentPage: () => currentPage,
    focusField: () => searchBar?.focus(),
    focusPaper: () => paper?.focus(),
    selectHit: (hit) => paper?.selectFound(hit),
    scrollToArea: (page, rect) => paper?.scrollToArea(page, rect),
    error: report,
  });

  const commenting = new Commenting({
    doc: () => doc,
    request: (kind) => paper?.commentRequest(kind),
    clearSelection: () => paper?.clearSelection(),
    reveal: (key) => {
      if (!list.visible) list.toggle();
      selectedKey = key;
    },
    message: (text) => (message = text),
    error: report,
  });

  // A new annotation can arrive after its editor opens and may not match
  // the active filters until its author has entered the text.
  $effect(() => {
    const key = commenting.editing?.key;
    if (key && listView.filtering && rows?.some((r) => r.key === key) && !shown.some((r) => r.key === key)) {
      listView.clear();
      message = "コメントを編集するため、絞り込みを解除しました";
    }
  });

  function onChanged(c: api.Changed) {
    if (!doc || c.generation !== doc.generation) return;
    for (const d of c.damage) recordDamage(c.generation, d.page, c.pageRevisions[d.page] ?? 0, d.rect);
    doc = { ...doc, dirty: c.dirty, pageRevisions: c.pageRevisions };
    loadComments();
  }

  const checking = new Checking({
    doc: () => doc,
    rows: () => rows,
    numbers: () => numbers,
    shown: () => shown,
    selectedKey: () => selectedKey,
    currentPage: () => currentPage,
    go: (key) => selectComment(key, true),
    message: (text) => (message = text),
    error: report,
  });

  const edits = new EditCommands({
    doc: () => doc,
    selectedText: () => selection.map((s) => s.text).join("\n"),
    copyFromList: () => {
      const row = selectedRow();
      if (!commentList?.hasFocus() || !row || row.editText === null) return false;
      void checking.copyEditText(row);
      return true;
    },
    message: (text) => (message = text),
    error: report,
  });

  function onWindowKey(e: KeyboardEvent) {
    if (!e.defaultPrevented && !e.isComposing && (e.ctrlKey || e.metaKey) && !e.altKey && !e.shiftKey
        && e.key.toLowerCase() === "c" && document.activeElement?.closest(".paper") && selection.length) {
      e.preventDefault();
      edits.copy();
      return;
    }
    if (doc && !e.defaultPrevented && !e.isComposing && (e.ctrlKey || e.metaKey) && !e.altKey && e.key.toLowerCase() === "f") {
      e.preventDefault();
      if (e.shiftKey) void withList((l) => l.focusSearch());
      else search.show();
      return;
    }
    windowKey(e, {
      selectedKey: () => selectedKey,
      remove: (key) => commenting.remove(key),
      edit: (key) => commenting.edit(key),
      toggle: (key) => checking.toggle(key),
      next: (step) => checking.next(step),
      deselect: () => (selectedKey = null),
    });
  }

  // The browser's own menu offers nothing here, such as saving the page as
  // an image; text fields keep theirs for cutting and pasting.
  function onContextMenu(e: MouseEvent) {
    if (!(e.target instanceof HTMLInputElement || e.target instanceof HTMLTextAreaElement)) e.preventDefault();
  }

  /** A right click on a row of the list: the row is selected and its commands shown. */
  function rowMenu(key: string) {
    const row = rows?.find((r) => r.key === key);
    if (!row || !doc) return;
    selectComment(key, true);
    showMenu(commentMenu(row, !doc.readOnlyReason), report);
  }

  /** Shows the list first; it is mounted only while shown, so `then` runs a tick later. */
  async function withList(then: (list: CommentList) => void) {
    if (!list.visible) list.toggle();
    await tick();
    if (commentList) then(commentList);
  }

  async function loadDocument() {
    try {
      const info = await api.documentInfo();
      selectedKey = null;
      flashKey = null;
      rows = null;
      panel = null;
      currentPage = 0;
      search.reset();
      listView.clear();
      selection = [];
      area = null;
      doc = info;
      const page = notices.opened(info);
      if (info) {
        if (page === 0) paper?.toTop();
        view.start(info);
        const painted = new Promise<void>((resolve) => {
          pagePainted = resolve;
          window.setTimeout(resolve, 300);
        });
        outlineItems = await api.outline();
        if (page > 0) {
          await tick();
          paper?.scrollToPage(page);
        }
        // The page first: laying out a long list keeps the window's thread
        // from putting the page's picture on screen.
        await painted;
        rows = await api.comments();
      } else {
        outlineItems = [];
        recent = await api.recentFiles();
      }
    } catch (e) {
      report(`文書の情報を読み込めませんでした: ${String(e)}`);
    } finally {
      starting = false;
    }
  }

  async function loadComments() {
    try {
      const fresh = await api.comments();
      rows = fresh && keepUnchanged(rows, fresh);
      if (doc && rows) doc = { ...doc, commentCount: rows.length };
      // The selected comment may have been removed.
      if (rows && selectedKey && !rows.some((r) => r.key === selectedKey)) selectedKey = null;
    } catch (e) {
      report(`コメント一覧を読み込めませんでした: ${String(e)}`);
    }
  }

  function goToPage(page: number) {
    if (!doc) return;
    paper?.scrollToPage(Math.min(doc.pages.length - 1, Math.max(0, page)));
  }

  let flashTimer = 0;
  function selectComment(key: string, fromList: boolean) {
    selectedKey = key;
    const row = rows?.find((r) => r.key === key);
    if (row && fromList) paper?.scrollToComment(row);
    flashKey = key;
    window.clearTimeout(flashTimer);
    flashTimer = window.setTimeout(() => (flashKey = null), 600);
  }

  const menu = menuCommands({
    edits,
    search,
    commenting,
    checking,
    list,
    view,
    selectedKey: () => selectedKey,
    selectedRow,
    open: (p) => (panel = p),
    hasOutline: () => outlineItems.length > 0,
    currentPage: () => currentPage,
    pageCount: () => doc?.pages.length ?? 0,
    goToPage,
    focusPageField: () => toolbar?.focusPage(),
    searchList: () => withList((l) => l.focusSearch()),
    filterList: () => withList((l) => l.focusFilter()),
    message: (text) => (message = text),
  });

  onMount(() => {
    // The application answers the requests of a starting window one at a
    // time: the settings of the list, which decide how wide the pages are
    // drawn, and the document are asked for ahead of all that can wait.
    list.load();
    const unlisten = [api.onDocumentChanged(loadDocument)];
    loadDocument();
    unlisten.push(
      api.onCommentsChanged(loadComments),
      api.onAnnotationsChanged(onChanged),
      api.onFileOutside((r) => notices.reported(r.generation, r.outside)),
      api.onRecoveryChanged(() => notices.loadOffers()),
      api.onMenu((id) => menu[id]?.()),
      api.onStatus((m) => {
        if (m.isError) error = m.text;
        else message = m.text;
      }),
    );
    notices.loadOffers();
    api
      .currentStatus()
      .then((m) => {
        if (m?.isError) error = m.text;
        else if (m) message = m.text;
      })
      .catch((e) => report(`ステータスバーの表示を読み込めませんでした: ${String(e)}`));
    const stopIdle = watchIdle(report);
    return () => {
      stopIdle();
      for (const u of unlisten) u.then((f) => f());
    };
  });
</script>

<svelte:window onkeydown={onWindowKey} oncontextmenu={onContextMenu} />

<div class="window">
  {#if doc}
    <Toolbar
      bind:this={toolbar}
      zoom={view.zoom}
      fit={view.fit}
      pageCount={doc.pages.length}
      {currentPage}
      hasOutline={outlineItems.length > 0}
      listVisible={list.visible}
      onFit={(mode) => view.fitTo(mode)}
      onZoom={(z) => view.to(z)}
      onZoomStep={(d) => view.to(nextZoom(view.zoom, d))}
      onGoToPage={goToPage}
      onOutline={() => (panel = "outline")}
      onToggleList={() => list.toggle()}
    />
    {#if search.open}
      <SearchBar
        bind:this={searchBar}
        query={search.query}
        current={search.index}
        total={search.done ? search.total : null}
        busy={search.busy}
        onQuery={(q) => search.input(q)}
        onStep={(d) => search.step(d)}
        onClose={() => search.close()}
      />
    {/if}
    <div class="main" style:grid-template-columns={list.visible ? `1fr 4px min(${list.width}px, 60%)` : "1fr"}>
      <Paper
        bind:this={paper}
        {doc}
        rows={rows ?? []}
        {numbers}
        zoom={view.zoom}
        {selectedKey}
        {flashKey}
        {search}
        onSelect={(key) => selectComment(key, false)}
        onDeselect={() => (selectedKey = null)}
        onError={report}
        onPainted={() => pagePainted()}
        onCurrentPage={(p) => (currentPage = p)}
        onViewport={(size) => (view.viewport = size)}
        onZoomRequest={(z, point) => view.to(z, point)}
        onSelectionChange={(s) => (selection = s)}
        onAreaChange={(a) => (area = a)}
        onCommand={(kind) => commenting.create(kind)}
      />
      {#if list.visible}
        <div
          class="splitter"
          role="separator"
          aria-orientation="vertical"
          aria-label="コメント一覧の幅"
          onpointerdown={(e) => list.startResize(e)}
        ></div>
        <CommentList
          bind:this={commentList}
          {rows}
          {shown}
          view={listView}
          {numbers}
          total={doc.commentCount}
          fontSize={list.fontSize}
          {selectedKey}
          editing={commenting.editing}
          editable={!doc.readOnlyReason}
          onSelect={(key) => selectComment(key, true)}
          onDeselect={() => (selectedKey = null)}
          onMenu={rowMenu}
          onEdit={(key) => commenting.edit(key)}
          onReply={(key) => commenting.reply(key)}
          onDelete={(key) => commenting.remove(key)}
          onToggle={(key) => checking.toggle(key)}
          onCopy={(row) => checking.copyEditText(row)}
          onCommit={(field, text) => commenting.commit(field, text)}
          onCancel={(field) => commenting.cancel(field)}
        />
      {/if}
    </div>
    {#if panel === "outline"}
      <OutlinePanel items={outlineItems} onGo={goToPage} onClose={() => (panel = null)} />
    {/if}
  {:else if !starting}
    <StartScreen {recent} onOpen={(path) => api.openFile(path).catch((e) => report(String(e)))} />
  {/if}
  {#if panel === "help"}
    <ShortcutHelp onClose={() => (panel = null)} />
  {:else if panel === "author"}
    <AuthorSettings onClose={() => (panel = null)} onDone={(text) => (message = text)} onError={report} />
  {:else if panel === "colors"}
    <CommentColors onClose={() => (panel = null)} onDone={(text) => (message = text)} onError={report} />
  {/if}
  <StatusBar
    {message}
    {error}
    notice={notices.current}
    {counts}
    selection={selectionLabel}
    dirty={doc && !doc.readOnlyReason ? doc.dirty : null}
    onDismissError={() => (error = null)}
    onSave={edits.save}
  />
</div>

<style>
  .window {
    position: relative;
    display: flex;
    flex-direction: column;
    height: 100%;
  }

  .window > :global(.main),
  .window > :global(.start) {
    flex: 1;
    min-height: 0;
  }

  .main {
    display: grid;
    min-height: 0;
  }

  .main > :global(*) {
    min-width: 0;
    min-height: 0;
  }

  .splitter {
    cursor: col-resize;
    background: var(--bg);
    border-left: 1px solid var(--line);
  }

  .splitter:hover {
    background: var(--hover);
  }
</style>
