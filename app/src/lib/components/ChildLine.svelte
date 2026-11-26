<!-- A side job the agent handed off, as one line in the parent: who is on it, what, and a way into its thread. Pictures it made show under it, read from the child's log; its answer folds open.
     Used by Transcript.svelte. -->
<script lang="ts">
  // A side job the agent handed off, as one line in the parent: who is on it, what, and a way into
  // its thread. Pictures it made show under it, read from the child's log; its answer folds open.
  import { hub } from "../state.svelte";
  import { engineProvider } from "../providers.svelte";
  import { diffLines, folded, jobWork, type ChildItem, type FileChange, type ImageItem } from "../transcript";
  import { duration, live as liveLabel, running as isRunning } from "../activity";
  import ImageCarousel from "./ImageCarousel.svelte";
  import Markdown from "./Markdown.svelte";
  import ProviderMark from "./ProviderMark.svelte";
  import Icon from "./Icon.svelte";
  import Working from "./Working.svelte";
  import { reveal } from "../menus.svelte";

  let { item, parent: parentId }: { item: ChildItem; parent?: string } = $props();

  // An engine by the name people use (Codex, not OpenAI), as the other rows in a thread say it.
  const byName = (engine: string) => {
    const p = engineProvider(engine);
    return /codex/i.test(engine) ? { ...p, name: "Codex" } : p;
  };
  const p = $derived(byName(item.engine));
  // Who handed the job over: the thread this line sits in, which stays on its own engine.
  const parent = $derived.by(() => {
    const e = hub.threads.find((t) => parentId && t.id === parentId)?.engine;
    return e ? byName(e) : null;
  });
  const VERBS: Record<string, [string, string]> = {
    image: ["drawing", "drew"],
    research: ["looking into", "looked into"],
    review: ["reviewing", "reviewed"],
  };
  const verb = $derived(VERBS[item.job] ?? ["working on", "did"]);
  const took = $derived(item.doneTs && item.doneTs > item.ts ? duration(item.doneTs - item.ts) : "");
  let open = $state(false);
  // What it changed and is doing now, read from its own log (followed while it works). Pictures,
  // research and reviews change no files, so only the other jobs are followed for this.
  const edits = $derived(!["image", "research", "review", "quick"].includes(item.job));
  $effect(() => {
    if (edits) hub.follow(item.id);
  });
  const work = $derived(edits ? jobWork(hub.events[item.id] ?? []) : { files: [] as FileChange[], step: null });
  const needsYou = $derived(hub.threads.find((t) => t.id === item.id)?.status === "waiting_on_you");
  const cwd = $derived(hub.threads.find((t) => t.id === item.id)?.cwd ?? "");
  const rel = (path: string) => {
    const base = cwd.replace(/[\\/]+$/, "");
    return (base && path.startsWith(base) ? path.slice(base.length).replace(/^[\\/]/, "") : path).replace(/\\/g, "/");
  };
  const stats = (f: FileChange) => {
    let add = 0, del = 0;
    for (const e of f.edits) for (const l of diffLines(e.old, e.new, 0)) l.op === "+" ? add++ : l.op === "-" ? del++ : 0;
    return { add, del };
  };
  const all = $derived(work.files.map(stats).reduce((a, s) => ({ add: a.add + s.add, del: a.del + s.del }), { add: 0, del: 0 }));
  const now = $derived(item.state === "working" && work.step && isRunning(work.step) ? liveLabel(work.step) : "");
  // The chip unfolds the files (B); a file unfolds its diff (C). Nothing shows until you ask.
  let files = $state(false);
  let peek = $state<string | null>(null);
  const peeked = $derived(work.files.find((f) => f.path === peek));

  // The pictures live in the child's log, with the data the parent's event leaves out. Its log is
  // followed like an open thread's, so a picture clicked here opens in the viewer beside this one.
  $effect(() => {
    if (item.state === "done" && item.images.length) hub.follow(item.id);
  });
  const images = $derived(
    item.state === "done" && item.images.length ? folded(hub.events[item.id] ?? []).filter((i): i is ImageItem => i.kind === "image") : [],
  );

  // Where the pictures were saved, as one quiet line with a way to the folder, so the agent need not
  // say it: the machine they are on, and the file (or how many). The folder opens only on this machine.
  const child = $derived(hub.threads.find((t) => t.id === item.id));
  const saved = $derived.by(() => {
    const first = item.state === "done" ? item.images[0] : undefined;
    if (!first) return null;
    const machine = child?.host ?? hub.localHost ?? "";
    const name = item.images.length > 1 ? `${item.images.length} pictures` : first.split(/[\\/]/).pop() ?? first;
    return { machine, name, path: first, local: !child?.host };
  });
</script>

<!-- A side job is a branch off the thread, not a change of the thread: a bracket in the left margin
     that goes out from the agent's words and comes back, with the job's own words inside it. The thread
     stays on its own engine the whole time, so this never uses the wavy line (that one says "the
     thread itself changed"). -->
<div class="child {item.state}">
  <i class="top"></i><i class="bottom"></i>
  <div class="line {item.state}">
    <Icon name="branch" size={12} />
    <span class="tag">Side job</span>
    <span class="who">
      {#if parent}<ProviderMark id={parent.id} size={13} />{parent.name} asked {/if}<ProviderMark id={p.id} size={13} />{p.name}
    </span>
    <span class="grow"></span>
    {#if work.files.length}
      <button class="chip" class:on={files} onclick={() => (files = !files)} aria-expanded={files} data-tip="What it changed so far">
        {work.files.length} {work.files.length === 1 ? "file" : "files"}<span class="add">+{all.add}</span><span class="del">−{all.del}</span>
      </button>
    {/if}
    {#if item.state === "working" && needsYou}
      <!-- A side job has no row in a list, so when it waits on you the parent's line says so. -->
      <span class="state need"><span class="dot waiting_on_you"></span>Needs you</span>
    {:else if item.state === "working"}
      <span class="state"><Working size={11} />{verb[0]}…</span>
    {:else if item.state === "done"}
      <span class="state ok"><Icon name="check" size={12} />Done{took ? ` · ${took}` : ""}</span>
    {:else}
      <span class="state bad"><Icon name="x" size={12} />Failed</span>
    {/if}
    <button class="link" onclick={() => hub.open(item.id)} data-tip="Open the side job's thread"><Icon name="popout" size={11} />Open</button>
  </div>
  <div class="title">
    {#if item.state === "done"}{verb[1]}{" "}{:else if item.state === "failed"}could not do{" "}{/if}“{item.title}”{#if item.state === "failed" && item.error}<span class="why"> · {item.error}</span>{/if}
  </div>
  {#if now}<div class="now mono">{now}</div>{/if}
  {#if files && work.files.length}
    <div class="files">
      {#each work.files as f (f.path)}
        {@const s = stats(f)}
        <button class="file" class:on={peek === f.path} onclick={() => (peek = peek === f.path ? null : f.path)} data-tip="Show what changed in it">
          <span class="path">{rel(f.path)}</span><span class="add">+{s.add}</span><span class="del">−{s.del}</span>
        </button>
      {/each}
      {#if peeked}
        {#each peeked.edits as e, i (i)}
          <pre>{#each diffLines(e.old, e.new) as l, k (k)}<span class="l {l.op === '+' ? 'plus' : l.op === '-' ? 'minus' : ''}">{l.op} {l.text}
</span>{/each}</pre>
        {/each}
      {/if}
    </div>
  {/if}
  {#if item.state === "done" && (images.length || item.reply)}
    <div class="result">
      <span class="label">Result</span>
      {#if item.reply && !item.images.length}
        <button class="link" onclick={() => (open = !open)} aria-expanded={open}>{open ? "Hide answer" : "Show answer"}</button>
      {/if}
    </div>
  {/if}
  {#if images.length}<ImageCarousel {images} threadId={item.id} />{/if}
  {#if saved}
    <div class="saved" data-tip={saved.path}>
      <span class="where">Saved on {saved.machine || "this machine"}</span>
      <span class="name mono">{saved.name}</span>
      {#if saved.local}<button class="link" onclick={reveal(saved.path)} data-tip="Show it in its folder"><Icon name="folder" size={11} />Folder</button>{/if}
    </div>
  {/if}
  {#if open && item.reply}<div class="reply"><Markdown text={item.reply} /></div>{/if}
</div>

<style>
  /* The bracket: a rail in the margin with a corner at each end, in the colour of side jobs. */
  .child { --c: var(--p-sky); position: relative; margin: 12px 0 12px 20px; padding: 2px 0 2px 14px; font-size: 13px; min-width: 0; }
  .child::before { content: ""; position: absolute; left: -14px; top: 9px; bottom: 9px; border-left: 1.5px solid var(--c); opacity: 0.55; }
  .top, .bottom { position: absolute; left: -14px; width: 16px; height: 10px; border: 0 solid var(--c); opacity: 0.55; }
  .top { top: 0; border-left-width: 1.5px; border-top-width: 1.5px; border-top-left-radius: var(--r-md); }
  .bottom { bottom: 0; border-left-width: 1.5px; border-bottom-width: 1.5px; border-bottom-left-radius: var(--r-md); }
  /* Finished: the bracket closes on a dot, the way a branch rejoins. */
  .child.done .bottom::after { content: ""; position: absolute; left: 14px; bottom: -3px; width: 5px; height: 5px; border-radius: 50%; background: var(--c); }
  .child.failed { --c: var(--red); }
  .line { display: flex; align-items: center; gap: 8px; color: var(--text-3); min-width: 0; flex-wrap: wrap; row-gap: 2px; }
  .line :global(svg:not(.mark)) { color: var(--c); flex: none; }
  .tag { color: var(--c); font-size: 12px; font-weight: 500; white-space: nowrap; }
  .who { display: inline-flex; align-items: center; gap: 4px; white-space: nowrap; }
  .grow { flex: 1; }
  .state { display: inline-flex; align-items: center; gap: 4px; color: var(--muted); font-size: 12px; white-space: nowrap; }
  .state.ok, .state.ok :global(svg) { color: var(--green); }
  .state.bad, .state.bad :global(svg) { color: var(--red); }
  .state.need { color: var(--amber); }
  .title { color: var(--text-2); margin: 3px 0 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; min-width: 0; }
  .why { color: var(--muted); }
  /* Working pulses green like every other working thing; amber is for what needs you. */
  .line .dot { margin: 0 1px; }
  .link { display: inline-flex; align-items: center; gap: 4px; color: var(--muted); font-size: 12px; padding: 1px 6px; border-radius: var(--r-sm); flex: none; }
  .link:hover { color: var(--accent); background: var(--accent-bg); }
  .line .link :global(svg) { color: inherit; }
  .result { display: flex; align-items: center; gap: 8px; margin: 8px 0 -2px; }
  .label { color: var(--muted); font-size: 12px; font-weight: 500; }
  .chip { display: inline-flex; align-items: center; gap: 6px; padding: 1px 8px; border: 1px solid var(--line); border-radius: var(--r-sm); color: var(--text-3); font-size: 12px; font-variant-numeric: tabular-nums; flex: none; }
  .chip:hover, .chip.on { background: var(--surface-hover); color: var(--text); }
  .add { color: var(--green); }
  .del { color: var(--red); }
  .now { margin: 2px 0 0; color: var(--muted); font-size: 12px; white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
  .mono { font-family: var(--mono); }
  .files { margin: 6px 0 2px; border: 1px solid var(--line); border-radius: var(--r-md); background: var(--surface); padding: 4px 0; display: flex; flex-direction: column; }
  .file { display: flex; align-items: center; gap: 10px; padding: 3px 12px; color: var(--text-3); font-family: var(--mono); font-size: 12px; text-align: left; }
  .file:hover, .file.on { background: var(--surface-hover); color: var(--text); }
  .file .path { flex: 1; min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .files pre { margin: 4px 12px 6px; padding: 6px 0; font-family: var(--mono); font-size: 12px; max-height: 280px; overflow: auto; border-top: 1px solid var(--line); }
  .l { display: block; color: var(--text-3); white-space: pre-wrap; }
  .plus { color: var(--green); background: var(--green-bg); }
  .minus { color: var(--red); background: var(--red-bg); }
  .saved { display: flex; align-items: center; gap: 8px; margin: 4px 0 0; color: var(--muted); font-size: 12px; min-width: 0; }
  .saved .where { white-space: nowrap; flex: none; }
  .saved .name { min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; color: var(--text-3); }
  .reply { user-select: text; -webkit-user-select: text; border-left: 1px solid var(--line-strong); margin: 4px 0 4px 6px; padding: 2px 0 2px 12px; line-height: 1.6; font-size: 14px; color: var(--text-2); }
</style>
