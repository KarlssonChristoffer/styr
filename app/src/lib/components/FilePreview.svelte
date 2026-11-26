<!-- One file shown as what it is (`filePreview.ts` says which kind): Markdown rendered, JSON as a tree, JSON Lines and CSV/TSV as tables, pictures, PDFs, sound and video as themselves, HTML as a still page, spreadsheets as a table per sheet, SQLite files as their tables, notebooks as cells, logs by level, env files with their values hidden, and code coloured. A small bar switches between the reading and the file's text. The text can be given (a reply's own edits); otherwise it is read from the folder, on whichever machine has it.
     Used by Changes.svelte, NamedFile.svelte, ProjectView.svelte, Viewer.svelte and 1 more. -->
<script lang="ts">
  import { quietly } from "../quiet";
  import Quiet from "./Quiet.svelte";
  // One file shown as what it is (`filePreview.ts` says which kind): Markdown rendered, JSON as a
  // tree, JSON Lines and CSV/TSV as tables, pictures, PDFs, sound and video as themselves, HTML as
  // a still page, spreadsheets as a table per sheet, SQLite files as their tables, notebooks as cells, logs by level, env files with their values hidden, and code
  // coloured. A small bar switches between the reading and the file's text. The text can be given
  // (a reply's own edits); otherwise it is read from the folder, on whichever machine has it.
  import { api } from "../api";
  import type { Database } from "../bindings/Database";
  import type { Workbook } from "../bindings/Workbook";
  import { hubdHas } from "../compat";
  import { envLines, fileKind, isBinary, parseDelimited, parseJsonl, parseLooseJson, sizeOf, tableOf, type FileKind } from "../filePreview";
  import CodeView from "./CodeView.svelte";
  import DataTable from "./DataTable.svelte";
  import DocReader from "./DocReader.svelte";
  import JsonTree from "./JsonTree.svelte";
  import LogView from "./LogView.svelte";
  import NotebookView from "./NotebookView.svelte";
  import PageFrame from "./PageFrame.svelte";

  let {
    root,
    rel,
    host = null,
    text: given = null,
    fill = false,
    tile = false,
    flat = false,
    ondoc,
  }: {
    /** The folder the file is in (a project, or the file's own folder). */
    root: string;
    /** The file, relative to `root`. */
    rel: string;
    host?: string | null;
    /** The file's text when it is already known. */
    text?: string | null;
    /** Take all the height there is (the viewer) instead of a scrolling box (the chat). */
    fill?: boolean;
    /** A still in the chat: no bar, nothing to click, a page drawn small; the card cuts it. */
    tile?: boolean;
    /** In the chat, unfolded: a live page is as tall as it says, so the chat scrolls and not a box in a box. */
    flat?: boolean;
    ondoc?: (doc: string, heading: string | null) => void;
  } = $props();

  const kind = $derived<FileKind>(fileKind(rel));
  // What each kind can be shown as; the first is how it opens.
  const MODES: Partial<Record<FileKind, string[]>> = {
    markdown: ["Read", "Source"],
    json: ["Tree", "Source"],
    jsonl: ["Table", "Lines", "Source"],
    csv: ["Table", "Source"],
    tsv: ["Table", "Source"],
    svg: ["Picture", "Source"],
    html: ["Page", "Source"],
    notebook: ["Cells", "Source"],
    log: ["Log", "Source"],
    env: ["Hidden", "Shown"],
  };
  const modes = $derived(MODES[kind] ?? []);
  let mode = $state("");
  $effect(() => {
    void rel;
    mode = MODES[fileKind(rel)]?.[0] ?? "";
  });
  const source = $derived(mode === "Source");

  // ---- Loading ----
  let text = $state<string | null>(null);
  let truncated = $state(false);
  let blob = $state<{ url: string; size: number } | null>(null);
  let error = $state("");
  const wantsText = $derived((!isBinary(kind) && kind !== "sheet" && kind !== "database") || source);
  $effect(() => {
    const at = { root, rel, host };
    error = "";
    text = given;
    truncated = false;
    if (!wantsText || given !== null) return;
    let gone = false;
    api
      .projectFile(at.root, at.rel, at.host)
      .then((f) => {
        if (gone) return;
        text = f.text;
        truncated = f.truncated;
      })
      .catch((e) => !gone && (error = e instanceof Error ? e.message : String(e)));
    return () => (gone = true);
  });
  $effect(() => {
    const at = { root, rel, host };
    if (!isBinary(fileKind(at.rel))) return;
    let gone = false;
    let url: string | null = null;
    blob = null;
    api
      .projectRaw(at.root, at.rel, at.host)
      .then((b) => {
        url = b.url;
        if (gone) URL.revokeObjectURL(b.url);
        else blob = b;
      })
      .catch((e) => !gone && (error = e instanceof Error ? e.message : String(e)));
    return () => {
      gone = true;
      if (url) URL.revokeObjectURL(url);
    };
  });

  // A spreadsheet is read by the core into one table per sheet, when this Hub can (`view.sheet`).
  let book = $state<Workbook | null>(null);
  let sheetAt = $state(0);
  let needsNext = $state(false);
  $effect(() => {
    const at = { root, rel, host };
    book = null;
    sheetAt = 0;
    needsNext = false;
    if (fileKind(at.rel) !== "sheet") return;
    let gone = false;
    (at.host ? Promise.resolve(true) : hubdHas("view.sheet")) // compat-until: 2026-12-03
      .then((has) => (has ? api.projectSheet(at.root, at.rel, at.host) : ((needsNext = true), null)))
      .then((b) => !gone && b && (book = b))
      .catch((e) => !gone && (error = e instanceof Error ? e.message : String(e)));
    return () => (gone = true);
  });
  const sheet = $derived.by(() => {
    const s = book?.sheets[sheetAt];
    if (!s) return null;
    const width = Math.max(0, ...s.rows.map((r) => r.length));
    const [head = [], ...rows] = s.rows;
    return { name: s.name, columns: Array.from({ length: width }, (_, i) => head[i] ?? ""), rows, cut: s.total_rows > s.rows.length };
  });

  // A SQLite file is read by the core into its tables, when this Hub can (`view.database`).
  let base = $state<Database | null>(null);
  let tableAt = $state(0);
  let baseNeedsNext = $state(false);
  $effect(() => {
    const at = { root, rel, host };
    base = null;
    tableAt = 0;
    baseNeedsNext = false;
    if (fileKind(at.rel) !== "database") return;
    let gone = false;
    (at.host ? Promise.resolve(true) : hubdHas("view.database")) // compat-until: 2026-12-06
      .then((has) => (has ? api.projectDatabase(at.root, at.rel, at.host) : ((baseNeedsNext = true), null)))
      .then((b) => !gone && b && (base = b))
      .catch((e) => !gone && (error = e instanceof Error ? e.message : String(e)));
    return () => (gone = true);
  });
  const dbTable = $derived(base?.tables[tableAt] ?? null);

  // An HTML file runs as a live page (scripts on, no network, like a page an agent showed) when the
  // core on this machine can serve it; otherwise, or on another machine, it stays a still picture.
  let live = $state<{ thread: string; page: string } | null>(null);
  /** Whether it is still being found out if the file can run as a page. */
  let asking = $state(false);
  $effect(() => {
    const at = { root, rel, host };
    live = null;
    asking = false;
    if (fileKind(at.rel) !== "html" || at.host) return;
    let gone = false;
    asking = true;
    hubdHas("pages.file") // compat-until: 2026-12-03
      .then((has) => (has ? api.projectPage(at.root, at.rel) : null))
      .then((r) => !gone && (live = r))
      .catch(quietly("live page"))
      .finally(() => !gone && (asking = false));
    return () => (gone = true);
  });

  // ---- Readings ----
  const json = $derived.by(() => {
    if (kind !== "json" || text === null) return null;
    try {
      return { value: parseLooseJson(text), error: null };
    } catch (e) {
      return { value: undefined, error: e instanceof Error ? e.message : String(e) };
    }
  });
  const lines = $derived(kind === "jsonl" && text !== null ? parseJsonl(text) : null);
  const linesTable = $derived(lines ? tableOf(lines.values) : null);
  const delimited = $derived.by(() => {
    if ((kind !== "csv" && kind !== "tsv") || text === null) return null;
    const rows = parseDelimited(text, kind === "tsv" ? "\t" : undefined);
    if (!rows.length) return { columns: [], rows: [] };
    const width = Math.max(...rows.map((r) => r.length));
    return { columns: Array.from({ length: width }, (_, i) => rows[0][i] ?? ""), rows: rows.slice(1) };
  });
  const env = $derived(kind === "env" && text !== null ? envLines(text) : null);
  const hide = (v: string) => (v ? "•".repeat(Math.min(12, Math.max(6, v.length))) : "");
  let dims = $state("");
  const nameOf = (p: string) => p.split(/[\\/]/).pop() ?? p;
</script>

<div class="preview" class:fill>
  {#if !tile && (modes.length || blob || truncated)}
    <div class="bar">
      {#each modes as m (m)}<button class:on={mode === m} onclick={() => (mode = m)}>{m}</button>{/each}
      <span class="info">
        {#if dims}{dims} · {/if}{#if blob}{sizeOf(blob.size)}{/if}{#if truncated}<span class="cut">only the first 512 KB</span>{/if}
      </span>
    </div>
  {/if}
  <div class="body">
    {#if error}
      <p class="none">{error}</p>
    {:else if isBinary(kind) && !source}
      {#if !blob}
        <Quiet kind="loading" />
      {:else if kind === "image" || kind === "svg"}
        <div class="picture"><img src={blob.url} alt={nameOf(rel)} onload={(e) => (dims = `${(e.currentTarget as HTMLImageElement).naturalWidth} × ${(e.currentTarget as HTMLImageElement).naturalHeight}`)} /></div>
      {:else if kind === "pdf"}
        <iframe class="pdf" src={blob.url} title={nameOf(rel)}></iframe>
      {:else if kind === "audio"}
        <audio controls src={blob.url}></audio>
      {:else if kind === "video"}
        <video controls src={blob.url}><track kind="captions" /></video>
      {/if}
    {:else if kind === "sheet"}
      {#if needsNext}
        <Quiet kind="newer" />
      {:else if !book || !sheet}
        {#if book}<Quiet text="This spreadsheet has no sheets" />{:else}<Quiet kind="loading" />{/if}
      {:else}
        {#if book.sheets.length > 1}
          <div class="sheets">{#each book.sheets as s, i (i)}<button class:on={sheetAt === i} onclick={() => (sheetAt = i)}>{s.name}</button>{/each}</div>
        {/if}
        {#if sheet.cut}<p class="warn">Only the first {sheet.rows.length + 1} rows are shown.</p>{/if}
        {#key sheetAt}<DataTable columns={sheet.columns} rows={sheet.rows} />{/key}
      {/if}
    {:else if kind === "database"}
      {#if baseNeedsNext}
        <Quiet kind="newer" />
      {:else if !base || !dbTable}
        {#if base}<Quiet text="This database has no tables" />{:else}<Quiet kind="loading" />{/if}
      {:else}
        <div class="sheets">{#each base.tables as t, i (i)}<button class:on={tableAt === i} onclick={() => (tableAt = i)}>{t.name}<span class="n">{t.total_rows}</span></button>{/each}</div>
        <p class="meta">{dbTable.columns.length} {dbTable.columns.length === 1 ? "column" : "columns"} · {dbTable.total_rows} {dbTable.total_rows === 1 ? "row" : "rows"}{#if dbTable.total_rows > dbTable.rows.length}, the first {dbTable.rows.length} shown{/if}</p>
        {#key tableAt}<DataTable columns={dbTable.columns} rows={dbTable.rows} />{/key}
      {/if}
    {:else if text === null}
      <Quiet kind="loading" />
    {:else if source || kind === "code" || kind === "text" || (kind === "json" && json?.error)}
      {#if json?.error && !source}<p class="warn">Not valid JSON ({json.error}), shown as text.</p>{/if}
      <CodeView path={kind === "text" || kind === "log" ? `${rel}.txt` : rel} {text} />
    {:else if kind === "markdown"}
      <DocReader {text} {rel} {ondoc} />
    {:else if kind === "json" && json}
      <div class="tree"><JsonTree value={json.value} /></div>
    {:else if kind === "jsonl" && lines}
      {#if lines.bad}<p class="warn">{lines.bad} {lines.bad === 1 ? "line does" : "lines do"} not read as JSON and {lines.bad === 1 ? "is" : "are"} left out.</p>{/if}
      {#if mode === "Table" && linesTable}
        <DataTable columns={linesTable.columns} rows={linesTable.rows} />
      {:else}
        <div class="tree">
          {#each lines.values.slice(0, 500) as v, i (i)}<JsonTree value={v} name={i} path={`[${i}]`} open={1} />{/each}
        </div>
      {/if}
    {:else if (kind === "csv" || kind === "tsv") && delimited}
      <DataTable columns={delimited.columns} rows={delimited.rows} />
    {:else if kind === "html"}
      {#if asking}
        <Quiet kind="loading" />
      {:else if live && tile}
        <div class="still"><PageFrame preview threadId={live.thread} id={live.page} title={nameOf(rel)} /></div>
      {:else if live}
        <PageFrame threadId={live.thread} id={live.page} title={nameOf(rel)} full={fill} grow={fill || flat} flush={flat} flow={flat} />
      {:else}
        <!-- No scripts, no forms, no same origin: a still picture of the page. -->
        <iframe class="page" sandbox="" srcdoc={text} title={nameOf(rel)}></iframe>
      {/if}
    {:else if kind === "notebook"}
      <NotebookView {text} />
    {:else if kind === "log"}
      <LogView {text} />
    {:else if kind === "env" && env}
      <pre class="env">{#each env as l, i (i)}{#if l.key}<span class="k">{l.key}</span><span class="d">=</span>{#if mode === "Shown"}<span class="val">{l.value}</span>{:else}<span class="d">{hide(l.value)}</span>{/if}{:else}<span class="d">{l.raw}</span>{/if}
{/each}</pre>
    {/if}
  </div>
</div>

<style>
  .preview { display: flex; flex-direction: column; min-height: 0; }
  .bar { display: flex; align-items: center; gap: 2px; padding: 0 0 8px; }
  .bar button { padding: 1px 8px; border-radius: var(--r-sm); color: var(--muted); font-size: 12px; }
  .bar button:hover { color: var(--text); }
  .bar button.on { color: var(--text); background: var(--surface-hover); }
  .info { margin-left: auto; color: var(--muted); font-size: 12px; font-variant-numeric: tabular-nums; }
  .cut { color: var(--amber); margin-left: 6px; }
  .body { user-select: text; -webkit-user-select: text; min-height: 0; display: flex; flex-direction: column; }
  .fill { flex: 1; }
  .fill .body { flex: 1; overflow: auto; }
  .still { position: relative; height: 236px; margin: -6px -12px; }
  .none { color: var(--muted); font-size: 13px; margin: 0; }
  .warn { color: var(--amber); font-size: 12px; margin: 0 0 6px; }
  .tree { overflow-x: auto; }
  .sheets { display: flex; gap: 2px; padding: 0 0 8px; overflow-x: auto; }
  .sheets button { flex: none; padding: 1px 8px; border-radius: var(--r-sm); color: var(--muted); font-size: 12px; }
  .sheets button:hover { color: var(--text); }
  .sheets .n { margin-left: 6px; color: var(--muted); font-variant-numeric: tabular-nums; }
  .meta { color: var(--muted); font-size: 12px; margin: 0 0 6px; }
  .sheets button.on { color: var(--text); background: var(--surface-hover); }
  .picture { display: grid; place-items: center; padding: 8px; border-radius: var(--r-md);
    background: repeating-conic-gradient(var(--surface) 0 25%, var(--bg-sidebar) 0 50%) 0 0 / 16px 16px; }
  .picture img { max-width: 100%; max-height: 60vh; object-fit: contain; }
  /* In the viewer the picture is fitted into all the room there is, centred, never past it. */
  .fill .picture { flex: 1; position: relative; min-height: 200px; padding: 0; }
  .fill .picture img { position: absolute; inset: 12px; margin: auto; max-width: calc(100% - 24px); max-height: calc(100% - 24px); }
  .pdf, .page { width: 100%; height: 60vh; border: 1px solid var(--line); border-radius: var(--r-md); background: #fff; }
  .fill .pdf, .fill .page { flex: 1; height: auto; min-height: 300px; }
  audio { width: 100%; }
  video { max-width: 100%; max-height: 60vh; border-radius: var(--r-md); background: #000; }
  .env { margin: 0; padding: 8px 12px; background: var(--bg-sidebar); border: 1px solid var(--line); border-radius: var(--r-md); font: 13px/1.6 var(--mono); white-space: pre-wrap; overflow-wrap: anywhere; }
  .env .k { color: var(--text); }
  .env .d { color: var(--muted); }
  .env .val { color: var(--green); }
</style>
