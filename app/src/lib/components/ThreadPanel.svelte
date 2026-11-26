<!-- The right panel in a thread: what the thread is doing and what it has touched. The agent's plan, every file it changed across all replies, its branch, the servers it started, and the connections (MCP servers, claude.ai connectors) its agent called or can call.
     Used by App.svelte. -->
<script lang="ts">
  import { quiet, quietly } from "../quiet";
  import { poll } from "../poll";
  import Quiet from "./Quiet.svelte";
  // The right panel in a thread: what the thread is doing and what it has touched. The agent's
  // plan, every file it changed across all replies, its branch, the servers it started, and the
  // connections (MCP servers, claude.ai connectors) its agent called or can call.
  import { api } from "../api";
  import type { GitInfo } from "../bindings/GitInfo";
  import type { Listener } from "../bindings/Listener";
  import type { Thread } from "../bindings/Thread";
  import type { ThreadChange } from "../bindings/ThreadChange";
  import type { ThreadConnections } from "../bindings/ThreadConnections";
  import { SOURCE, STATE } from "../connections";
  import { openUrl } from "../native";
  import { menu } from "../menu.svelte";
  import { fileMenu, firstCheckpoint, serverMenu } from "../menus.svelte";
  import { hub, isStandIn } from "../state.svelte";
  import { preview } from "../preview";
  import { diffLines, folded, latestShown, shownIn, shownName, type FileChange } from "../transcript";
  import Icon from "./Icon.svelte";
  import TerminalOutput from "./TerminalOutput.svelte";
  import TerminalPane from "./TerminalPane.svelte";
  import { commands, type ToolItem } from "../transcript";
  import { duration, failedStep } from "../activity";
  import { prefs, setPref } from "../prefs.svelte";
  import { tick as settled, untrack } from "svelte";
  import { openPicture } from "../lightbox.svelte";
  import { ago } from "../format";
  import type { Output } from "../bindings/Output";
  import { openBig } from "../viewkit/bigview.svelte";
  import { kindOf } from "../viewkit/kinds";
  import ViewRow from "../viewkit/Row.svelte";

  let { thread }: { thread: Thread } = $props();

  const items = $derived(folded(hub.events[thread.id] ?? []));
  const plan = $derived(items.findLast((i) => i.kind === "plan")?.entries ?? []);
  const planDone = $derived(plan.filter((p) => p.status === "completed").length);

  // Edits from every reply, one row per file, in the order they were first touched.
  const edited = $derived.by(() => {
    const byPath = new Map<string, FileChange>();
    for (const i of items) {
      if (i.kind !== "changes") continue;
      for (const f of i.files) {
        const seen = byPath.get(f.path);
        if (seen) seen.edits = [...seen.edits, ...f.edits];
        else byPath.set(f.path, { path: f.path, edits: [...f.edits] });
      }
    }
    return [...byPath.values()];
  });

  // What git sees changed since the thread started, which also counts files a command wrote and
  // commits the agent made. Asked again after every turn; null outside git.
  let changed = $state<ThreadChange[] | null>(null);
  $effect(() => {
    if (thread.status === "running") return;
    const id = thread.id;
    quiet(api.changes(id).then((c) => thread.id === id && (changed = c)), "changes");
  });

  type Line = { op: string; text: string };
  type Row = { path: string; add: number; del: number; hunks: Line[][] };
  const unified = (diff: string): Line[] =>
    diff
      .split("\n")
      .filter((l) => l && !/^(diff --git|index |--- |\+\+\+ |new file|deleted file|similarity|rename |old mode|new mode)/.test(l))
      .map((l) => (l.startsWith("@@") ? { op: "@", text: l } : { op: l[0] === "+" || l[0] === "-" ? l[0] : " ", text: l.slice(1) }));
  const files = $derived<Row[]>(
    changed
      ? changed.map((c) => ({ path: c.path, add: c.added, del: c.removed, hunks: [unified(c.diff)] }))
      : edited.map((f) => {
          const hunks = f.edits.map((e) => diffLines(e.old, e.new));
          let add = 0, del = 0;
          for (const e of f.edits) for (const l of diffLines(e.old, e.new, 0)) l.op === "+" ? add++ : l.op === "-" ? del++ : 0;
          return { path: f.path, add, del, hunks };
        }),
  );
  const rel = (path: string) => {
    const base = thread.cwd.replace(/[\\/]+$/, "");
    const p = path.startsWith(base) ? path.slice(base.length).replace(/^[\\/]/, "") : path;
    return p.replace(/\\/g, "/");
  };
  const name = (p: string) => p.split("/").at(-1) ?? p;
  const dir = (p: string) => p.split("/").slice(0, -1).join("/");

  // What the agent showed, newest first, a changed page once at its newest version; each opens
  // in the viewer.
  const all = $derived(shownIn(items));
  const shown = $derived(latestShown(all));

  // The docs and files the thread wrote, from the project's library (`outputs.rs`), asked again
  // after every turn. Pages and pictures come from the log above, so they show at once.
  let made = $state<Output[]>([]);
  $effect(() => {
    if (thread.status === "running" || isStandIn(thread.id)) return;
    const id = thread.id;
    api.projectOutputs(thread.cwd, thread.host)
      .then((o) => thread.id === id && (made = o.filter((i) => i.thread === id && (i.kind === "doc" || i.kind === "file") && i.open.path)))
      .catch(quietly("thread outputs"));
  });

  // Not the engine's own port: stopping that would end the thread's engine.
  const servers = $derived((hub.host?.listeners ?? []).filter((l) => l.thread_id === thread.id && l.origin !== "engine"));

  let git = $state<GitInfo | null>(null);
  $effect(() => {
    void thread.status;
    const id = thread.id;
    // A thread still being made (`hub.showStarting`) has no folder to ask about yet.
    if (isStandIn(id)) return;
    const load = () => quiet(api.git(id).then((g) => (git = g)), "git");
    load();
    const timer = poll(load, 10_000);
    return () => timer();
  });

  // Asking the engine checks every server, so this loads once per thread and again after a turn;
  // the core keeps the engine's answer for a while.
  let links = $state<ThreadConnections | null>(null);
  let showAll = $state(false);
  $effect(() => {
    if (thread.status === "running") return;
    const id = thread.id;
    quiet(api.threadConnections(id).then((c) => thread.id === id && (links = c)), "thread connections");
  });
  const reachable = $derived((links?.available ?? []).filter((c) => !c.off && c.state !== "off" && c.state !== "failed"));

  // A second opinion on everything changed since the thread started.
  const reviewing = $derived(items.some((i) => i.kind === "review" && i.state === "running"));
  const canReview = $derived(!!firstCheckpoint(thread.id));
  async function secondOpinion() {
    try {
      await api.review(thread.id);
    } catch (e) {
      hub.fail(e);
    }
  }

  // Sections fold to one line that still says how things stand; which are shut is remembered.
  const shut = (key: string) => prefs.panelShut.includes(key);
  const flip = (key: string) => setPref("panelShut", shut(key) ? prefs.panelShut.filter((k) => k !== key) : [...prefs.panelShut, key]);

  // Commands the agent ran: running and failed ones in sight, finished ones behind one line.
  const ran = $derived(commands(items));
  const busy = (c: ToolItem) => c.status === "in_progress" || c.status === "pending";
  const live = $derived(ran.filter(busy));
  const bad = $derived(ran.filter((c) => !busy(c) && failedStep(c)));
  const done = $derived(ran.filter((c) => !busy(c) && !failedStep(c)));
  let showDone = $state(false);
  let unfolded = $state<string | null>(null);
  const line = (c: ToolItem) => c.command ?? c.title.replace(/^Run\s+/, "").replace(/`/g, "");
  let now = $state(Date.now());
  $effect(() => {
    if (!live.length) return;
    const timer = poll(() => (now = Date.now()), 1000);
    return () => timer();
  });
  const clock = (c: ToolItem) => (c.since ? duration((c.until ?? now) - c.since) : "");
  let runBox = $state<HTMLElement>();
  // Opened at a command (from the chat or the status line): unfold it and bring it into view.
  // Each ask counts, the same command twice too; commands finishing later do not unfold it again.
  $effect(() => {
    void hub.terminalAsked;
    const id = hub.terminalFocus;
    if (!id) return;
    unfolded = id;
    if (untrack(() => done.some((c) => c.id === id))) showDone = true;
    settled().then(() => runBox?.querySelector(`[data-call="${CSS.escape(id)}"]`)?.scrollIntoView({ block: "nearest" }));
  });

  // Take over: the thread's session in its engine's own CLI, in a terminal inside Hub
  // (`hubd attach`, over SSH for another machine). One that still runs is found again after the
  // app reloads.
  const takeover = $derived(hub.takeovers[thread.id] ?? null);
  let takeoverEnded = $state<number | null>(null);
  let takeoverBox = $state<HTMLElement>();
  $effect(() => {
    const id = thread.id;
    if (untrack(() => hub.takeovers[id]) || isStandIn(id)) return;
    api
      .terminals()
      .then((all) => {
        const t = all.find((x) => x.what === "attach" && x.thread === id && x.exit === null);
        if (t && !hub.takeovers[id]) hub.takeovers = { ...hub.takeovers, [id]: t.id };
      })
      .catch(quietly("takeover terminals"));
  });
  $effect(() => {
    void takeover;
    takeoverEnded = null;
  });
  $effect(() => {
    if (!hub.takeoverAsked || !takeover) return;
    settled().then(() => takeoverBox?.scrollIntoView({ block: "nearest" }));
  });

  const adds = $derived(files.reduce((n, f) => n + f.add, 0));
  const dels = $derived(files.reduce((n, f) => n + f.del, 0));

  let open = $state<string | null>(null);
  let stopping = $state<number | null>(null);
  async function stop(l: Listener) {
    if (l.pid == null) return;
    stopping = l.pid;
    try {
      await api.stopProcess(l.pid);
      await hub.refreshHost();
    } catch (e) {
      hub.fail(e);
    } finally {
      stopping = null;
    }
  }
</script>

{#snippet head(key: string, title: string, summary: import("svelte").Snippet)}
  <button class="sh" class:shut={shut(key)} aria-expanded={!shut(key)} onclick={() => flip(key)}>
    <span class="chev"><Icon name="chevron" size={11} /></span>
    <span class="st">{title}</span>
    <span class="sum">{@render summary()}</span>
  </button>
{/snippet}

<div class="panel">
  {#if takeover}
    <section bind:this={takeoverBox}>
      {#snippet termSum()}
        {#if takeoverEnded !== null}Ended{:else if thread.status === "in_terminal"}<span class="dot running"></span>Has the thread{:else}Following{/if}
      {/snippet}
      {@render head("takeover", "Terminal", termSum)}
      {#if !shut("takeover")}
        <div class="sb">
          <div class="tbox">
            {#key takeover}<TerminalPane id={takeover} onexit={(code) => (takeoverEnded = code)} />{/key}
          </div>
          <div class="tfoot">
            <span class="quiet-note">{thread.status === "in_terminal" ? "Exit the CLI to hand the thread back." : thread.status === "running" || thread.status === "waiting_on_you" ? "Enter stops the turn and takes it over." : "Enter takes it over."}</span>
            <button class="more" onclick={() => hub.closeTakeover(thread.id)}>Close</button>
          </div>
        </div>
      {/if}
    </section>
  {/if}

  {#if plan.length}
    <section>
      {#snippet planSum()}{planDone} of {plan.length}{/snippet}
      {@render head("plan", "Plan", planSum)}
      {#if !shut("plan")}
        <div class="sb">
          {#each plan as p, i (i)}
            <div class="step {p.status}">
              <span class="mark">
                {#if p.status === "completed"}<Icon name="check" size={12} />{:else if p.status === "in_progress"}<span class="dot running"></span>{:else}<span class="ring"></span>{/if}
              </span>
              <span class="text">{p.content}</span>
            </div>
          {/each}
        </div>
      {/if}
    </section>
  {/if}

  {#if ran.length || servers.length}
    <section bind:this={runBox}>
      {#snippet runSum()}
        {#if live.length || servers.length}<span class="dot running"></span>{live.length + servers.length} running{/if}
        {#if bad.length}<span class="dot failed"></span>{bad.length} failed{/if}
        {#if !live.length && !servers.length && !bad.length}{done.length} done{/if}
      {/snippet}
      {@render head("run", "Running", runSum)}
      {#if !shut("run")}
        <div class="sb">
          {#each servers as l (`${l.port}-${l.pid}`)}
            <div class="server" data-tip={l.exe ?? ""} use:menu={() => serverMenu(l)}>
              <span class="dot running"></span>
              <button
                class="port mono"
                data-tip="Open beside the thread (Ctrl-click: in your browser)"
                onclick={(e) => (e.ctrlKey || e.metaKey ? openUrl(`http://localhost:${l.port}`) : preview(`http://localhost:${l.port}`, thread.id))}>:{l.port}</button>
              <span class="proc">{l.process ?? "unknown"}</span>
              {#if l.pid != null && !l.is_hub}
                <button class="btn danger" disabled={stopping === l.pid} onclick={() => stop(l)}>Stop</button>
              {/if}
            </div>
          {/each}
          {#snippet cmd(c: ToolItem)}
            {@const failed = !busy(c) && failedStep(c)}
            <div class="cmd" class:open={unfolded === c.id} data-call={c.id}>
              <button class="crow" onclick={() => (unfolded = unfolded === c.id ? null : c.id)} data-tip={line(c)}>
                <span class="dot" class:running={busy(c)} class:failed></span>
                <span class="cline mono">{line(c)}</span>
                <span class="cend" class:bad={failed}>{[c.exit != null && c.exit !== 0 ? `exit ${c.exit}` : c.status === "failed" ? "failed" : "", clock(c)].filter(Boolean).join(" · ")}</span>
              </button>
              {#if unfolded === c.id}
                <div class="cout">
                  {#if c.output}<TerminalOutput text={c.output} cut={c.cut} max={400} scroll />{:else}<span class="quiet">{busy(c) ? "No output yet" : "No output"}</span>{/if}
                </div>
              {/if}
            </div>
          {/snippet}
          {#each live as c (c.id)}{@render cmd(c)}{/each}
          {#each bad as c (c.id)}{@render cmd(c)}{/each}
          {#if done.length}
            <button class="more" onclick={() => (showDone = !showDone)}>{showDone ? "Hide" : "Show"} {done.length} finished</button>
            {#if showDone}{#each [...done].reverse() as c (c.id)}{@render cmd(c)}{/each}{/if}
          {/if}
        </div>
      {/if}
    </section>
  {/if}

  <section>
    {#snippet fileSum()}
      {#if files.length}<span class="add">+{adds}</span><span class="del">−{dels}</span>{/if}{files.length}
      {#if git?.changed}<span class="amber" data-tip="Uncommitted in the folder">{git.changed} uncommitted</span>{/if}
    {/snippet}
    {@render head("files", "Changed files", fileSum)}
    {#if !shut("files")}
      <div class="sb">
        <div class="frow">
          {#if git?.branch}<span class="branch mono" data-tip="Git branch"><Icon name="branch" size={11} />{git.branch}</span>{/if}
          {#if canReview && files.length}
            <button class="eye" data-tip={reviewing ? "Reviewing…" : "Get a second opinion on everything the thread changed"} disabled={reviewing} onclick={secondOpinion}><Icon name="eye" size={13} /></button>
          {/if}
        </div>
        {#each files as f (f.path)}
          {@const r = rel(f.path)}
          <button class="file" class:open={open === f.path} data-tip={r} onclick={() => (open = open === f.path ? null : f.path)} use:menu={() => fileMenu(f.path, { rel: r, remote: !!thread.host })}>
            <span class="fname">{name(r)}{#if dir(r)}<span class="fdir">{dir(r)}</span>{/if}</span>
            <span class="add">+{f.add}</span><span class="del">−{f.del}</span>
          </button>
          {#if open === f.path}
            {#each f.hunks as lines, i (i)}
              <pre>{#each lines as l, k (k)}<span class="l {l.op === '+' ? 'plus' : l.op === '-' ? 'minus' : l.op === '@' ? 'hunk' : ''}">{l.op === "@" ? l.text : `${l.op} ${l.text}`}
</span>{/each}</pre>
            {/each}
          {/if}
        {:else}
          <Quiet text="Nothing changed" />
        {/each}
      </div>
    {/if}
  </section>

  {#if links && (links.used.length || reachable.length)}
    <section>
      {#snippet linkSum()}{links!.used.length} used{#if reachable.length}<span>· {reachable.length} reachable</span>{/if}{/snippet}
      {@render head("connections", "Connections", linkSum)}
      {#if !shut("connections")}
        <div class="sb">
          {#each links.used as c (c.name)}
            <div class="link" data-tip="{SOURCE[c.source]} · {STATE[c.state].label}{c.target ? ` · ${c.target}` : ''}">
              <span class="ldot {STATE[c.state].tone}"></span>
              <span class="lname">{c.label}</span>
              <span class="lcount">{c.uses}</span>
            </div>
            <div class="ltools">
              {#each c.tools as t (t.name)}<span class="mono">{t.name}{#if t.count > 1}<span class="times"> ×{t.count}</span>{/if}</span>{/each}
            </div>
          {/each}
          {#if reachable.length}
            <button class="more" onclick={() => (showAll = !showAll)}>
              {links.used.length ? "Can also reach" : "Can reach"} {reachable.length}{showAll ? "" : `: ${reachable.slice(0, 3).map((c) => c.label).join(", ")}${reachable.length > 3 ? "…" : ""}`}
            </button>
            {#if showAll}
              {#each reachable as c (c.name)}
                <div class="link" data-tip="{STATE[c.state].label}{c.target ? ` · ${c.target}` : ''}">
                  <span class="ldot {STATE[c.state].tone}"></span>
                  <span class="lname dim">{c.label}</span>
                  <span class="lsource">{SOURCE[c.source]}</span>
                </div>
              {/each}
              <button class="more" onclick={() => hub.openSettings("extensions")}>All connections in Settings</button>
            {/if}
          {/if}
        </div>
      {/if}
    </section>
  {/if}

  {#if shown.length || made.length}
    <!-- What the agent made to look at is not part of the work above: the thread's library. -->
    <div class="group">Library</div>
    <section>
      {#snippet docSum()}{shown.length + made.length}{/snippet}
      {@render head("docs", "Made in this thread", docSum)}
      {#if !shut("docs")}
        <div class="sb">
          {#each [...shown].reverse() as s (`${s.kind}:${s.id}`)}
            {#if s.kind === "image"}
              <ViewRow kind="picture" name={shownName(s, all)} thumb={s.src} onopen={() => openPicture(thread.id, s.id)} />
            {:else}
              <ViewRow kind="page" name={s.title} detail={s.of > 1 ? `v${s.of}` : ""} onopen={() => openBig(thread.id, s.id)} />
            {/if}
          {/each}
          {#each made as o (o.id)}
            <ViewRow kind={kindOf(o.kind)} name={o.title} detail={ago(o.changed_at)} tip="Open" onopen={() => o.open.path && (hub.viewing = { thread: thread.id, kind: "file", id: o.open.path })} />
          {/each}
        </div>
      {/if}
    </section>
  {/if}
</div>

<style>
  .panel { padding: 4px 0 16px; overflow-y: auto; min-height: 0; flex: 1; }
  section { border-bottom: 1px solid var(--line); }
  /* A section's line: its name, and on the right how things stand, also when it is folded. */
  .sh { display: flex; align-items: center; gap: 8px; width: 100%; padding: 8px 18px; text-align: left; color: var(--text); font-size: 13px; }
  .sh:hover { background: var(--surface); }
  .sh .chev { display: grid; color: var(--muted); transform: rotate(90deg); transition: transform 0.12s var(--ease); }
  .sh.shut .chev { transform: none; }
  .sh.shut .st { color: var(--text-3); }
  .st { font-weight: 500; white-space: nowrap; }
  .sum { margin-left: auto; display: inline-flex; align-items: center; gap: 6px; color: var(--muted); font-size: 12px; font-variant-numeric: tabular-nums; white-space: nowrap; min-width: 0; overflow: hidden; }
  .sum .dot { width: 7px; height: 7px; }
  .amber { color: var(--amber); }
  .sb { padding: 0 18px 12px; }
  .group { color: var(--muted); font-size: 12px; padding: 22px 18px 6px; }
  .dot { width: 7px; height: 7px; border-radius: 50%; flex: none; background: var(--line-strong); }
  .dot.failed { background: var(--red); }
  .frow { display: flex; align-items: center; gap: 8px; margin: -2px 0 4px; }
  .frow:empty { display: none; }
  .cmd { margin: 0 -6px; border-radius: var(--r-sm); }
  .cmd.open { background: var(--surface); }
  .crow { display: flex; align-items: center; gap: 8px; width: 100%; padding: 4px 6px; border-radius: var(--r-sm); text-align: left; }
  .crow:hover { background: var(--surface); }
  .cline { flex: 1; min-width: 0; color: var(--text-2); font-size: 12px; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .cend { color: var(--muted); font-size: 12px; white-space: nowrap; font-variant-numeric: tabular-nums; }
  .cend.bad { color: var(--red); }
  .cout { padding: 2px 8px 8px 21px; }
  .tbox { height: min(460px, 56vh); }
  .tfoot { display: flex; align-items: center; gap: 8px; padding-top: 6px; }
  .quiet-note { flex: 1; min-width: 0; color: var(--muted); font-size: 12px; }
  .quiet { color: var(--muted); font-family: var(--mono); font-size: 12px; font-style: italic; }
  .branch { display: inline-flex; align-items: center; gap: 4px; color: var(--muted); font-size: 12px; font-weight: 400; max-width: 50%; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .uncommitted { color: var(--amber); font-size: 12px; margin: -2px 0 6px; }
  .step { display: flex; gap: 8px; align-items: flex-start; padding: 3px 0; font-size: 13px; line-height: 1.45; color: var(--text-2); }
  .mark { width: 14px; height: 19px; flex: none; display: grid; place-items: center; color: var(--muted); }
  .ring { width: 8px; height: 8px; border-radius: 50%; border: 1.5px solid var(--muted); }
  .step.completed .text { color: var(--muted); text-decoration: line-through; text-decoration-color: var(--line-strong); }
  .step.in_progress .text { color: var(--text); }
  .file { display: flex; align-items: baseline; gap: 6px; width: calc(100% + 12px); margin: 0 -6px; padding: 4px 6px; border-radius: var(--r-sm); text-align: left; font-size: 13px; }
  .file:hover, .file.open { background: var(--surface); }
  .fname { flex: 1; min-width: 0; color: var(--text-2); font-family: var(--mono); overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .fdir { color: var(--muted); margin-left: 6px; }
  .add { color: var(--green); font-size: 12px; font-variant-numeric: tabular-nums; }
  .del { color: var(--red); font-size: 12px; font-variant-numeric: tabular-nums; }
  pre { margin: 2px 0 8px; padding: 6px 0; font-family: var(--mono); font-size: 12px; max-height: 260px; overflow: auto; background: var(--surface); border-radius: var(--r-sm); }
  .l { display: block; color: var(--text-3); white-space: pre-wrap; padding: 0 8px; }
  .plus { color: var(--green); background: var(--green-bg); }
  .minus { color: var(--red); background: var(--red-bg); }
  .hunk { color: var(--muted); }
  .server { display: flex; align-items: center; gap: 8px; padding: 4px 0; }
  .port { color: var(--text); background: var(--surface); padding: 1px 6px; border-radius: var(--r-sm); font-size: 12px; min-width: 58px; text-align: center; }
  .port:hover { background: var(--surface-hover); }
  .proc { flex: 1; min-width: 0; color: var(--text-2); font-size: 13px; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .btn { height: 24px; padding: 0 8px; font-size: 12px; }
  .none { color: var(--muted); font-size: 13px; margin: 4px 0; }
  .link { display: flex; align-items: center; gap: 8px; padding: 3px 0; font-size: 13px; }
  .ldot { width: 6px; height: 6px; border-radius: 50%; flex: none; background: var(--muted); }
  .ldot.ok { background: var(--green); }
  .ldot.warn { background: var(--amber); }
  .ldot.bad { background: var(--red); }
  .lname { flex: 1; min-width: 0; color: var(--text-2); overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .lname.dim { color: var(--muted); }
  .lcount, .lsource { color: var(--muted); font-size: 12px; font-variant-numeric: tabular-nums; white-space: nowrap; }
  .ltools { display: flex; flex-wrap: wrap; gap: 2px 8px; padding: 0 0 4px 14px; font-size: 12px; color: var(--text-3); }
  .times { color: var(--muted); }
  .more { display: block; color: var(--muted); font-size: 12px; padding: 4px 0; text-align: left; }
  .more:hover { color: var(--text-2); }
  .eye { display: grid; place-items: center; width: 20px; height: 20px; border-radius: var(--r-sm); color: var(--muted); align-self: center; }
  .eye:hover { color: var(--text); background: var(--surface-hover); }
  .eye:disabled { opacity: 0.4; }
</style>
