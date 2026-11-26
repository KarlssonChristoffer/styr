<!-- The footer of one reply: the files it changed with their +/−, whether it showed that it works, the second opinion on it, copy and the time. A file opens below the line, as it reads (FilePreview) or as its diff; File and Diff switch, Wide opens it in the viewer.
     Used by Transcript.svelte. -->
<script lang="ts">
  // The footer of one reply: the files it changed with their +/−, whether it showed that it works,
  // the second opinion on it, copy and the time. A file opens below the line, as it reads
  // (FilePreview) or as its diff; File and Diff switch, Wide opens it in the viewer.
  import { diffLines, type FileChange } from "../transcript";
  import Icon from "./Icon.svelte";
  import IconSwap from "./IconSwap.svelte";
  import { copy, menu } from "../menu.svelte";
  import { fileMenu } from "../menus.svelte";

  import { api } from "../api";
  import { clock } from "../format";
  import { hub } from "../state.svelte";
  import type { DocsState, ProofState, ReviewItem } from "../transcript";
  import DocsStamp from "./DocsStamp.svelte";
  import ProofStamp from "./ProofStamp.svelte";
  import FilePreview from "./FilePreview.svelte";
  import NamedFile from "./NamedFile.svelte";
  import { fileKind, fromEdits, hasReader, placeOf, showsInChat } from "../filePreview";
  import { grown, showFile, toggleGrown, unfolded, unfoldKey } from "../viewkit/openfile.svelte";
  import Card from "../viewkit/Card.svelte";
  import { noteMissingViewer } from "../viewerGaps.svelte";
  import Reactions from "./Reactions.svelte";
  import { emojis, togglePicker } from "../reactions.svelte";

  let {
    files,
    cwd,
    threadId,
    review,
    proof,
    docs,
    answer = "",
    ts,
    seq,
  }: {
    files: FileChange[];
    cwd: string;
    threadId?: string;
    /** The latest second opinion that covers this reply, if any. */
    review?: ReviewItem;
    /** Whether the reply showed that it works. */
    proof?: ProofState;
    /** Whether it kept the docs that cover its code in step. */
    docs?: DocsState;
    /** The reply's last text, for copying. */
    answer?: string;
    /** When the reply ended. */
    ts?: number;
    /** The turn end that closes the reply: what a reaction to it points at. */
    seq?: number;
  } = $props();

  // Reviewing (orange) → fixing what it found (orange) → reviewed (green).
  const stage = $derived(
    !review ? null
    : review.state === "running" ? { tone: "busy", text: "reviewing…", title: `${review.model} is reviewing this` }
    : review.sent && !review.fixed ? { tone: "busy", text: "fixing review findings…", title: "The agent is working on what the review found" }
    : { tone: "done", text: review.sent ? "reviewed · fixed" : "reviewed", title: review.sent ? "Reviewed, and the agent has answered the findings" : "Reviewed, nothing to fix" },
  );
  let asked = $state(false);
  async function askReview() {
    if (!threadId || asked) return;
    asked = true;
    try {
      await api.review(threadId);
    } catch (err) {
      asked = false;
      hub.fail(err);
    }
  }

  let copied = $state(false);
  function copyAnswer() {
    copy(answer);
    copied = true;
    setTimeout(() => (copied = false), 1200);
  }

  const rel = (path: string) => {
    const base = cwd.replace(/[\\/]+$/, "");
    const p = path.startsWith(base) ? path.slice(base.length).replace(/^[\\/]/, "") : path;
    return p.replace(/\\/g, "/");
  };
  const stats = (f: FileChange) => {
    let add = 0, del = 0;
    for (const e of f.edits) for (const l of diffLines(e.old, e.new, 0)) l.op === "+" ? add++ : l.op === "-" ? del++ : 0;
    return { add, del };
  };

  // Up to three files by name; more fold into one count that opens the whole list.
  const FEW = 3;
  let all = $state(false);
  let opened = $state<string | null>(null);
  const listed = $derived(all || files.length <= FEW ? files : files.slice(0, FEW - 1));
  const rest = $derived(files.slice(listed.length));
  const restStats = $derived(rest.map(stats).reduce((a, s) => ({ add: a.add + s.add, del: a.del + s.del }), { add: 0, del: 0 }));
  const open = $derived(files.find((f) => f.path === opened));

  // An opened file reads as what it is (FilePreview) or as its diff. Kinds that read differently
  // from their text (Markdown, JSON, CSV, notebooks, …) open as they read; code opens as its diff.
  // The text comes from the reply's own edits when it wrote the whole file, so an old reply shows
  // what it wrote then; otherwise from the folder as it is now.
  let view = $state<"read" | "diff">("diff");
  const host = $derived(hub.threads.find((t) => t.id === threadId)?.host ?? null);
  const place = $derived(open ? placeOf(open.path, cwd) : null);
  const ownText = $derived(open ? fromEdits(open) : null);
  const inProject = $derived(!!place && place.root === cwd);
  $effect(() => {
    for (const f of files) noteMissingViewer(f.path, threadId);
  });
  // A table, a drawing or a page the reply wrote whole is shown right in the chat, above the footer.
  const SHOW = 2;
  const made = $derived(files.filter((f) => showsInChat(fileKind(f.path)) && fromEdits(f) != null).slice(0, SHOW));
  // The same kinds, and spreadsheets, when the reply only names the file (a script wrote it, not an
  // edit): found in the answer's words, shown if the folder has it (`NamedFile`).
  const NAMED = 4;
  const NAMED_RE = /[^\s`"'<>()[\]|*?]+\.(?:csv|tsv|svg|html?|xlsx|xlsm|xls|ods)\b/gi;
  const base = (p: string) => p.split(/[\\/]/).pop()!.toLowerCase();
  const named = $derived.by(() => {
    const have = new Set(files.map((f) => base(f.path)));
    // The same file named twice (a link's words, then its address) is one, by its fullest path.
    const found = new Map<string, string>();
    for (const m of answer.matchAll(NAMED_RE)) {
      const p = m[0].replace(/^file:\/+(?=[A-Za-z]:)/i, "").replace(/^file:\/\//i, "");
      if (p.includes("://") || p.startsWith("www.") || have.has(base(p))) continue;
      let path = p;
      try {
        path = decodeURIComponent(p);
      } catch {
        /* as written */
      }
      const had = found.get(base(path));
      if (!had || path.length > had.length) found.set(base(path), path);
    }
    return [...found.values()].slice(0, NAMED);
  });
  // Many outputs in one answer are rows with a miniature, not a preview each.
  const compact = $derived(made.length + named.length > 2);
  function toggle(path: string) {
    opened = opened === path ? null : path;
    view = hasReader(fileKind(path)) ? "read" : "diff";
  }
  // A link to another doc, or the Docs button, opens the project's docs tab there.
  function inDocs(doc: string, heading: string | null = null) {
    hub.openProject(cwd, host);
    hub.projectTab = "docs";
    hub.projectDoc = heading ? `${doc}#${heading}` : doc;
  }
  function wide(path: string) {
    if (threadId) hub.viewing = { thread: threadId, kind: "file", id: path };
  }
</script>

{#each made as f (f.path)}
  {@const at = placeOf(f.path, cwd)}
  <Card
    kind="file"
    name={rel(f.path)}
    {compact}
    onexpand={threadId ? () => showFile(threadId, f.path, fromEdits(f), { card: !compact }) : undefined}
    onpanel={threadId ? () => showFile(threadId, f.path, null, { how: "panel" }) : undefined}
    onmodal={threadId ? () => showFile(threadId, f.path, fromEdits(f), { how: "modal" }) : undefined}
    unfolded={!!threadId && !!unfolded[unfoldKey(threadId, f.path)]}
    flat={fileKind(f.path) === "html"}
    grown={!!threadId && !!grown[unfoldKey(threadId, f.path)]}
    ongrow={threadId ? () => toggleGrown(threadId, f.path) : undefined}
    items={() => fileMenu(f.path, { rel: rel(f.path), remote: !!host })}
  >
    {#key f.path}<FilePreview tile root={at.root} rel={at.rel} {host} text={fromEdits(f)} />{/key}
    {#snippet full()}<FilePreview flat root={at.root} rel={at.rel} {host} text={fromEdits(f)} />{/snippet}
  </Card>
{/each}

{#each named as p (p)}
  <NamedFile path={p} {cwd} {threadId} {host} {answer} {compact} />
{/each}

<div class="foot" class:quiet={!files.length}>
  {#if threadId && seq}<Reactions {threadId} {seq} align="start" />{/if}
  <div class="row">
    <span class="files">
      {#each listed as f (f.path)}
        {@const s = stats(f)}
        <button class="file" class:on={opened === f.path} data-tip="{rel(f.path)} · {hasReader(fileKind(f.path)) ? 'read it' : 'show the diff'}" use:menu={() => fileMenu(f.path, { rel: rel(f.path), remote: !!host })} onclick={() => toggle(f.path)}>
          <span class="path">{rel(f.path).split("/").pop()}</span>{#if s.add}<span class="add">+{s.add}</span>{/if}{#if s.del}<span class="del">−{s.del}</span>{/if}
        </button>
      {/each}
      {#if rest.length}
        <button class="file" data-tip={rest.map((f) => rel(f.path)).join("\n")} onclick={() => (all = true)}>
          {rest.length} more <span class="add">+{restStats.add}</span><span class="del">−{restStats.del}</span>
        </button>
      {/if}
    </span>
    {#if proof}<ProofStamp {proof} />{/if}
    {#if docs}<DocsStamp {docs} />{/if}
    {#if stage}
      <span class="stage {stage.tone}" data-tip={stage.title}>
        {#if stage.tone === "busy"}<span class="pulse"></span>{:else}<Icon name="check" size={11} />{/if}{stage.text}
      </span>
    {:else if threadId && files.length}
      <button class="icon" data-tip="Get a second opinion on everything since the last one" disabled={asked} onclick={askReview}><Icon name="eye" size={13} /></button>
    {/if}
    {#if answer}
      <button class="icon" data-tip={copied ? "Copied" : "Copy the reply"} onclick={copyAnswer}><IconSwap name={copied ? "check" : "copy"} size={13} /></button>
    {/if}
    {#if threadId && seq && emojis.on}<button class="icon" data-react-btn data-tip="React" aria-label="React" onclick={(e) => togglePicker(e.currentTarget, threadId, seq)}><Icon name="smilePlus" size={13} /></button>{/if}
    {#if ts}<span class="time">{clock(ts)}</span>{/if}
  </div>
  {#if open}
    <div class="diff">
      <div class="where">
        <span class="mono">{rel(open.path)}</span>
        <span class="modes">
          <button class:on={view === "read"} onclick={() => (view = "read")}>File</button>
          <button class:on={view === "diff"} onclick={() => (view = "diff")}>Diff</button>
          {#if threadId}<button data-tip="Open it wide beside the thread" onclick={() => wide(open.path)}>Wide</button>{/if}
          {#if fileKind(open.path) === "markdown" && inProject}<button data-tip="Open it in the project's Docs tab" onclick={() => inDocs(place!.rel)}>Docs</button>{/if}
        </span>
      </div>
      {#if view === "read" && place}
        <div class="read">
          {#key open.path}<FilePreview root={place.root} rel={place.rel} {host} text={ownText} ondoc={inProject ? inDocs : undefined} />{/key}
        </div>
      {:else}
        {#each open.edits as e, i (i)}
          <pre>{#each diffLines(e.old, e.new) as l, k (k)}<span class="l {l.op === '+' ? 'plus' : l.op === '-' ? 'minus' : ''}">{l.op} {l.text}
</span>{/each}</pre>
        {/each}
      {/if}
    </div>
  {/if}
</div>

<style>
  .foot { margin: 2px 0 14px; font-size: 12px; color: var(--muted); }
  .row { display: flex; align-items: center; gap: 10px; min-height: 24px; border-top: 1px solid var(--line); padding-top: 3px; }
  .foot.quiet { margin: -6px 0 6px; }
  .quiet .row { border-top-color: transparent; min-height: 22px; }
  .quiet .icon, .quiet .time { opacity: 0; transition: opacity 0.12s var(--ease); }
  .quiet:hover .icon, .quiet:hover .time, .quiet .icon:focus-visible { opacity: 1; }
  .files { flex: 1; min-width: 0; display: flex; flex-wrap: wrap; gap: 2px 4px; }
  .file { display: inline-flex; align-items: center; gap: 6px; padding: 1px 6px; margin-left: -6px; border-radius: var(--r-sm); color: var(--text-3); font-family: var(--mono); font-size: 12px; white-space: nowrap; }
  .file + .file { margin-left: 0; }
  .file:hover, .file.on { background: var(--surface-hover); color: var(--text); }
  .path { max-width: 220px; overflow: hidden; text-overflow: ellipsis; }
  .add { color: var(--green); font-variant-numeric: tabular-nums; }
  .del { color: var(--red); font-variant-numeric: tabular-nums; }
  .stage { display: inline-flex; align-items: center; gap: 4px; font-size: 12px; white-space: nowrap; }
  .stage.done { color: var(--green); }
  .stage.busy { color: var(--amber); }
  .pulse { width: 6px; height: 6px; border-radius: 50%; background: var(--amber); }
  .icon { display: grid; place-items: center; width: 22px; height: 22px; border-radius: var(--r-sm); color: var(--muted); }
  .icon:hover { color: var(--text); background: var(--surface-hover); }
  .icon:disabled { opacity: 0.4; }
  .time { font-size: 12px; font-variant-numeric: tabular-nums; }
  .diff { background: var(--surface); border-radius: var(--r-md); margin-top: 6px; padding: 6px 0 2px; }
  .where { display: flex; align-items: center; gap: 10px; color: var(--text-3); padding: 0 12px 4px; font-size: 12px; }
  .where .mono { flex: 1; min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .modes { display: inline-flex; gap: 2px; }
  .modes button { padding: 1px 8px; border-radius: var(--r-sm); color: var(--muted); font-size: 12px; }
  .modes button:hover { color: var(--text); }
  .modes button.on { color: var(--text); background: var(--surface-hover); }
  /* A rendered file reads like a document, in its own scroll. */
  .read { margin: 0 12px 8px; padding: 10px 2px 6px; border-top: 1px solid var(--line); max-height: 70vh; overflow: auto; }
  pre { margin: 0 12px 8px; padding: 6px 0; font-family: var(--mono); font-size: 12px; max-height: 320px; overflow: auto; border-top: 1px solid var(--line); }
  .l { display: block; color: var(--text-3); white-space: pre-wrap; }
  .plus { color: var(--green); background: var(--green-bg); }
  .minus { color: var(--red); background: var(--red-bg); }
</style>
