<!-- One problem beside the list: Guard's reading on top (what, likely cause, plan, next), its stamps to correct, what to do with it, then the suspect commits, what happened to it and its tags. Hits, the code, problems like it and the whole activity log have tabs of their own.
     Used by ProblemsView.svelte. -->
<script lang="ts">
  import { quiet } from "../quiet";
  import { pop } from "../popover";
  // One problem beside the list: Guard's reading on top (what, likely cause, plan, next), its
  // stamps to correct, what to do with it, then the suspect commits, what happened to it and its
  // tags. Hits, the code, problems like it and the whole activity log have tabs of their own.
  import { api } from "../api";
  import type { Archive } from "../bindings/Archive";
  import type { Dossier } from "../bindings/Dossier";
  import type { Lane } from "../bindings/Lane";
  import type { Prio } from "../bindings/Prio";
  import type { Problem } from "../bindings/Problem";
  import type { ProblemDetail } from "../bindings/ProblemDetail";
  import type { ProblemPatch } from "../bindings/ProblemPatch";
  import { ago, clock, shortVersion } from "../format";
  import { SOURCES } from "../problems";
  import { openUrl } from "../native";
  import { hub } from "../state.svelte";
  import { guardName } from "../system";
  import { fixState, startFix } from "../fixes.svelte";
  import Icon from "./Icon.svelte";
  import Tabs from "./Tabs.svelte";
  import Select from "./Select.svelte";
  import Ref from "./Ref.svelte";
  import { machineGlyph } from "../refs";
  import type { Frame } from "../bindings/Frame";

  let { id, host = null, onclose, onopen }: { id: number; host?: string | null; onclose: () => void; onopen: (id: number) => void } = $props();

  let detail = $state<ProblemDetail | null>(null);
  let dossier = $state<Dossier | null>(null);
  let tab = $state<"overview" | "hits" | "code" | "similar" | "activity">("overview");
  let busy = $state(false);
  let mergeInto = $state("");
  let menu = $state<"" | "resolve" | "archive">("");
  /** Bumped when a change fails, so the category list shows what is saved, not the failed pick. */
  let failures = $state(0);

  const CATEGORIES = ["bug", "config", "environment", "external", "flaky", "performance", "security", "noise", "expected"];
  const LANES: [Lane, string][] = [["quiet", "Quiet"], ["tell", "Do and tell"], ["ask", "Ask first"]];
  const PRIOS: Prio[] = ["P0", "P1", "P2", "P3"];

  // An answer for a problem no longer shown (picked another one fast) is dropped.
  const still = (i: number, h: string | null) => i === id && h === host;
  async function load() {
    const [i, h] = [id, host];
    try {
      const d = await api.problem(i, h);
      if (still(i, h)) detail = d;
    } catch (e) {
      if (still(i, h)) hub.fail(e);
    }
    api.problemDossier(i, h).then((d) => still(i, h) && (dossier = d)).catch(() => still(i, h) && (dossier = null));
  }
  $effect(() => {
    void id;
    void host;
    tab = "overview";
    detail = null;
    dossier = null;
    load();
  });
  // Again when a problem changes anywhere, so Guard's work shows as it happens.
  $effect(() => {
    if (!hub.problemsTick) return;
    const [i, h] = [id, host];
    const t = setTimeout(() => quiet(api.problem(i, h).then((d) => still(i, h) && (detail = d)), "problem"), 500);
    return () => clearTimeout(t);
  });

  async function change(patch: ProblemPatch) {
    busy = true;
    menu = "";
    try {
      const [i, h] = [id, host];
      const p = await api.changeProblem(i, patch, h);
      if (detail && still(i, h)) detail = { ...detail, problem: p };
      await load();
    } catch (e) {
      failures++;
      hub.fail(e);
    } finally {
      busy = false;
    }
  }

  /** Hands the problem to a background fix thread; you stay here (`fixes.svelte.ts`). */
  function thread(p: Problem, plan: string | null, e: MouseEvent) {
    if (p.thread && hub.threads.some((t) => t.id === p.thread)) return hub.open(p.thread);
    void startFix(p.id, plan, host, e.currentTarget as Element);
  }

  function archive(a: Archive) {
    change({ archive: a });
  }

  const when = (ms: number) => (Date.now() - ms < 20 * 3600_000 ? clock(ms) : ago(ms));
  const p = $derived(detail?.problem);
  const hit = $derived(detail?.hits[0]);
  const release = $derived(detail?.hits.find((h) => h.release)?.release);
  const extra = $derived(hit && Object.keys(hit.extra ?? {}).length ? JSON.stringify(hit.extra, null, 2) : null);
  const tags = $derived.by(() => {
    const by: Record<string, string[]> = {};
    for (const t of detail?.tags ?? []) (by[t.key] ??= []).push(`${t.value} (${t.count})`);
    return Object.entries(by);
  });
  const archiveText = (a: Archive) =>
    a.until === "worse" ? "until it gets worse" : a.until === "count" ? `until ${a.count} times` : a.until === "time" ? `until ${when(a.at)}` : "for good";
  // The latest hit, for the Overview: the stack with runs of library frames folded, newest call
  // first as in Sentry, and the thread it happened in when a thread or a tool call sent it.
  type StackLine = { frame: Frame } | { libs: number };
  const stack = $derived.by((): StackLine[] => {
    // A frame that is only a name (a tool call's "claude · Bash") is no stack to show.
    if (!hit?.frames.some((f) => f.line != null || f.function)) return [];
    const frames = [...hit.frames].reverse().slice(0, 12);
    const out: StackLine[] = [];
    for (const f of frames) {
      const last = out.at(-1);
      if (!f.own && last && "libs" in last) last.libs++;
      else if (!f.own && out.some((l) => "frame" in l && l.frame.own)) out.push({ libs: 1 });
      else out.push({ frame: f });
    }
    return out;
  });
  const traceLines = $derived((hit?.trace ?? "").split("\n").filter((l) => l.trim()).slice(0, 8));
  const hitThread = $derived.by(() => {
    const id = hit?.extra?.thread;
    if (typeof id !== "string") return null;
    const t = hub.threads.find((x) => x.id === id);
    return { id, title: t?.title ?? (typeof hit?.extra?.title === "string" ? hit.extra.title : "its thread"), status: t?.status ?? "idle", known: !!t };
  });
  const hitInput = $derived(typeof hit?.extra?.input === "string" ? hit.extra.input : null);
  const crumbs = $derived((hit?.crumbs ?? []).slice(-10));
  const machineName = $derived(host ?? p?.machine ?? "");
  const hasThread = $derived(!!p?.thread && hub.threads.some((t) => t.id === p.thread));
</script>

{#if p && detail}
  <aside class="detail">
    <div class="head">
      {#if p.prio}<span class="prio {p.prio}">{p.prio}</span>{/if}
      <span class="lvl {p.level}"></span>
      <span class="headtext">
        {p.level === "error" ? "Error" : p.level === "warning" ? "Warning" : "Info"} · {p.project} · {p.machine}{release ? ` · ${release}` : ""} · first seen
        {when(p.first_seen)}{p.status !== "open" ? ` · ${p.status}` : ""}
      </span>
      <button class="icon close" aria-label="Close" onclick={onclose}><Icon name="x" size={13} /></button>
    </div>
    <h3>{p.title}</h3>
    {#if p.culprit}<div class="mono where-line">{p.culprit}</div>{/if}
    {#if p.merged_into}
      <p class="say">Merged into <button class="link" onclick={() => onopen(p.merged_into!)}>#{p.merged_into}</button>; its hits count there.
        <button class="link" onclick={() => change({ unmerge: true })}>Split it off again</button></p>
    {/if}

    <Tabs style="margin: 14px 0 12px" value={tab} onchange={(id) => (tab = id)} items={[
      { id: "overview", label: "Overview" },
      { id: "hits", label: "Hits", count: detail.hits.length },
      { id: "code", label: "Code" },
      { id: "similar", label: "Similar", count: dossier?.similar.length || null },
      { id: "activity", label: "Activity" },
    ]} />

    {#if tab === "overview"}
      <section class="vakten">
        <div class="vhead">
          <span class="who">{guardName(p.stamped_by ?? "Vakten")}</span>
          {#if p.note}
            <span>· read it{p.confidence ? ` · sure: ${p.confidence}` : ""}</span>
          {:else if p.stamped_by}
            <span>· stamped it</span>
          {:else}
            <span>· has not read it yet</span>
          {/if}
        </div>
        {#if p.note}
          {#if p.note.what}<p><b>What:</b> {p.note.what}</p>{/if}
          {#if p.note.cause}<p><b>Likely cause:</b> {p.note.cause}</p>{/if}
          {#if p.note.plan}<p><b>Plan:</b> {p.note.plan}</p>{/if}
          {#if p.note.next}<p><b>Next:</b> {p.note.next}</p>{/if}
        {/if}
        <div class="chips">
          {#key failures}
            <Select size="small" label="Category" placeholder="category" value={p.category ?? undefined} onchange={(category) => change({ category })}
              options={CATEGORIES.map((c) => ({ value: c, label: c }))} />
          {/key}
          {#if p.regressed}<span class="chip warn">came back</span>{/if}
          {#if p.escalating}<span class="chip warn">escalating</span>{/if}
          {#if p.fixable}<span class="chip">fixable: {p.fixable}</span>{/if}
          {#if p.archive && p.status === "archived"}<span class="chip">archived {archiveText(p.archive)}</span>{/if}
          {#if p.next_release && p.status === "resolved"}<span class="chip">resolved in next release</span>{/if}
          {#if p.pr}<button class="chip pr" onclick={() => openUrl(p.pr!.url)}>PR #{p.pr.number} · {p.pr.state}</button>{/if}
        </div>
        <div class="seg-row">
          <div class="seg" role="group" aria-label="Prio">
            {#each PRIOS as x (x)}<button class:on={p.prio === x} disabled={busy} onclick={() => change({ prio: x })}>{x}</button>{/each}
          </div>
          <div class="seg" role="group" aria-label="Lane">
            {#each LANES as [x, label] (x)}<button class:on={p.lane === x} disabled={busy} onclick={() => change({ lane: x })}>{label}</button>{/each}
          </div>
        </div>
      </section>

      <div class="acts">
        {#if hasThread}
          <button class="btn primary" onclick={() => hub.open(p.thread!)}>Open its thread</button>
        {:else if fixState(p.id, host)}
          <span class="sendoff {fixState(p.id, host)}" data-tip="A fix thread works on it in the background; it ends at a pull request">
            <span class="dot"></span>{fixState(p.id, host) === "sending" ? "Sending" : "In Inbox, working"}</span>
        {:else if p.note?.plan}
          <button class="btn primary" disabled={busy} onclick={(e) => thread(p, p.note!.plan, e)} data-tip="A background thread carries out Guard's plan and ends at a pull request; you stay here">Let Guard do it</button>
          <button class="btn" disabled={busy} onclick={(e) => thread(p, null, e)}>Start a thread myself</button>
        {:else}
          <button class="btn primary" disabled={busy} onclick={(e) => thread(p, null, e)} data-tip="A background thread fixes it on a small model and never asks you; you stay here">Start a thread on it</button>
        {/if}
        {#if p.status === "open"}
          {#if p.source === "code_analysis"}
            <span class="chip">Awaiting verification by a complete analysis</span>
          {:else}
          <span class="menuwrap">
            <button class="btn" onclick={() => (menu = menu === "resolve" ? "" : "resolve")}>Resolve <Icon name="chevron" size={11} /></button>
            {#if menu === "resolve"}
              <div class="menu" transition:pop|global>
                <button onclick={() => change({ resolve: "now" })}>Resolve now<small>comes back if it happens again</small></button>
                <button onclick={() => change({ resolve: "next_release" })}>Resolve in next release<small>hits from the releases it was seen in do not count</small></button>
              </div>
            {/if}
          </span>
          {/if}
          <span class="menuwrap">
            <button class="btn" onclick={() => (menu = menu === "archive" ? "" : "archive")}>Archive <Icon name="chevron" size={11} /></button>
            {#if menu === "archive"}
              <div class="menu" transition:pop|global>
                <button onclick={() => archive({ until: "worse", rate: 0 })}>Until it gets worse<small>back when it happens clearly more than now</small></button>
                <button onclick={() => archive({ until: "count", count: p.count + 100 })}>Until 100 more times</button>
                <button onclick={() => archive({ until: "time", at: Date.now() + 24 * 3600_000 })}>For a day</button>
                <button onclick={() => archive({ until: "time", at: Date.now() + 7 * 24 * 3600_000 })}>For a week</button>
                <button onclick={() => archive({ until: "forever" })}>For good<small>keeps counting, never shows as open</small></button>
              </div>
            {/if}
          </span>
        {:else}
          <button class="btn" onclick={() => change({ status: "open" })}>Open again</button>
        {/if}
        {#if p.new}<button class="btn ghost" onclick={() => change({ reviewed: true })} data-tip="No longer marked new">Seen</button>{/if}
      </div>

      {#if hit}
        <div class="label">Last time it happened</div>
        <div class="facts">
          <span>{when(hit.ts)} · {ago(hit.ts)}</span>
          {#if machineName}<span class="sep">·</span><Ref kind={machineGlyph(machineName, !host)} label={machineName} online={hub.hostInfo(machineName)?.state === "online" || !host} />{/if}
          {#if hit.release}<span class="sep">·</span><span>{hit.release}</span>{/if}
          {#if hit.environment}<span class="sep">·</span><span>{hit.environment}</span>{/if}
          {#if hitThread}
            <span class="sep">·</span>
            {#if hitThread.known}
              <button class="tref" onclick={() => hub.open(hitThread.id)} data-tip="Open the thread it happened in"><span class="dot {hitThread.status}"></span>{hitThread.title}</button>
            {:else}
              <span class="tref"><span class="dot idle"></span>{hitThread.title}</span>
            {/if}
          {/if}
        </div>
        {#if hit.message && hit.message.trim() !== p.title.trim()}<pre class="msg">{hit.message}</pre>{/if}
        {#if hitInput}
          <div class="label">What the agent sent</div>
          <pre class="msg">{hitInput}</pre>
        {/if}
        {#if stack.length || traceLines.length}
          <div class="label">Stack <button class="link more" onclick={() => (tab = "code")}>whole stack and commits</button></div>
          <div class="stack">
            {#if stack.length}
              {#each stack as l, i (i)}
                {#if "libs" in l}
                  <div class="frame lib">… {l.libs} library {l.libs === 1 ? "frame" : "frames"}</div>
                {:else}
                  <div class="frame" class:own={l.frame.own}>
                    <span class="file">{l.frame.file ?? "?"}</span>{#if l.frame.function}<span class="fn">{" "}in {l.frame.function}</span>{/if}{#if l.frame.line}<span class="ln">:{l.frame.line}</span>{/if}
                    {#if l.frame.code && l.frame.own}<div class="code">{l.frame.code.trim()}</div>{/if}
                  </div>
                {/if}
              {/each}
            {:else}
              {#each traceLines as line, i (i)}<div class="frame" class:own={i < 2}>{line.trim()}</div>{/each}
            {/if}
          </div>
        {/if}
        {#if crumbs.length}
          <div class="label">Before it</div>
          <div class="trail">
            {#each crumbs as c, i (i)}
              <div class="crumb">
                <span class="time">{c.ts ? clock(c.ts) : ""}</span>
                <span class="dot {c.level === 'error' ? 'error' : c.level === 'warning' || c.level === 'warn' ? 'waiting_on_you' : 'idle'}"></span>
                <span class="grow">{#if c.category}<span class="cat">{c.category}</span>{/if}{c.message}</span>
              </div>
            {/each}
            <div class="crumb now">
              <span class="time">{clock(hit.ts)}</span>
              <span class="dot error"></span>
              <span class="grow">this error</span>
            </div>
          </div>
        {/if}
      {/if}

      {#if dossier?.suspects.length}
        <div class="label">Suspect commits</div>
        <div class="list">
          {#each dossier.suspects.slice(0, 4) as s (s.sha)}
            <div class="line"><span class="mono">{shortVersion(s.sha)}</span><span class="grow">{s.subject} · {ago(s.ts)} · {s.author}</span></div>
          {/each}
        </div>
      {/if}
      {#if detail.activity.length}
        <div class="label">Activity</div>
        <div class="list">
          {#each detail.activity.slice(0, 6) as a, i (i)}
            <div class="line"><span class="time">{when(a.ts)}</span><span class="grow"><b class="by">{guardName(a.by)}</b> {a.text}</span></div>
          {/each}
        </div>
      {/if}
      {#if tags.length}
        <div class="label">Tags</div>
        <div class="list">
          {#each tags as [k, values] (k)}
            <div class="line"><span class="time">{k}</span><span class="grow">{values.join(" · ")}</span></div>
          {/each}
          <div class="line"><span class="time">machine</span><span class="grow">{p.machine}</span></div>
          <div class="line"><span class="time">source</span><span class="grow">{SOURCES[p.source] ?? p.source}</span></div>
        </div>
      {/if}
      <form class="merge" onsubmit={(e) => (e.preventDefault(), mergeInto && change({ merge_into: Number(mergeInto), why: "the same error" }), (mergeInto = ""))}>
        <input class="field small" placeholder="Merge into #" inputmode="numeric" bind:value={mergeInto} />
        <button class="btn" disabled={!/^\d+$/.test(mergeInto)}>Merge</button>
      </form>
    {:else if tab === "hits"}
      {#each detail.hits as h, i (i)}
        <div class="label">{when(h.ts)}{h.release ? ` · ${h.release}` : ""}{h.environment ? ` · ${h.environment}` : ""}</div>
        {#if h.message && (i > 0 || h.message.trim() !== p.title.trim())}<pre>{h.message}</pre>{/if}
        {#if h.crumbs.length}
          <pre>{#each h.crumbs.slice(-12) as c, j (j)}<span class="crumb {c.level}">{c.ts ? clock(c.ts) : ""} {(c.level ?? "").toUpperCase().padEnd(5)} {c.message}
</span>{/each}</pre>
        {/if}
      {/each}
      {#if extra}
        <details>
          <summary class="label">More from the latest</summary>
          <pre>{extra}</pre>
        </details>
      {/if}
    {:else if tab === "code"}
      {#if hit?.frames.length}
        <div class="label">Stack, where it broke last</div>
        <pre>{#each hit.frames.slice(-20) as f, i (i)}<span class:lib={!f.own}>{f.file ?? "?"}{f.function ? ` in ${f.function}` : ""}{f.line ? `, line ${f.line}` : ""}
</span>{#if f.code}<span class="code">    {f.code}
</span>{/if}{/each}</pre>
      {:else if hit?.trace}
        <div class="label">Stack</div>
        <pre>{hit.trace}</pre>
      {:else}
        <p class="say">No stack.</p>
      {/if}
      <div class="label">Suspect commits{dossier?.folder ? ` in ${dossier.folder}` : ""}</div>
      {#if dossier?.suspects.length}
        <div class="list">
          {#each dossier.suspects as s (s.sha)}
            <div class="line"><span class="mono">{shortVersion(s.sha)}</span><span class="grow">{s.subject}<br /><small>{ago(s.ts)} · {s.author} · {s.files.join(", ")}</small></span></div>
          {/each}
        </div>
      {:else}
        <p class="say">{dossier?.folder ? "No commits touched the files in its stack." : "Hub does not know the folder of its code."}</p>
      {/if}
    {:else if tab === "similar"}
      {#if dossier?.similar.length}
        <div class="list">
          {#each dossier.similar as s (s.id)}
            <div class="line">
              <span class="time">#{s.id}</span>
              <button class="grow link left" onclick={() => onopen(s.id)}>{s.title}<br /><small>{s.project} · {s.status} · {s.count}× · {s.why}</small></button>
              {#if p.source !== "code_analysis"}<button class="btn small" onclick={() => change({ merge_into: s.id, why: s.why })} data-tip="This one's hits count there from now on">Merge into it</button>{/if}
            </div>
          {/each}
        </div>
      {:else}
        <p class="say">None.</p>
      {/if}
      <div class="label">Around the same time, anywhere</div>
      {#if dossier?.around.length}
        <pre>{#each dossier.around as a, i (i)}<span class="crumb {a.level}">{clock(a.ts)} {a.machine} {a.project}/{SOURCES[a.source] ?? a.source}: {a.message}
</span>{/each}</pre>
      {:else}
        <p class="say">None within five minutes.</p>
      {/if}
    {:else}
      <div class="list">
        {#each detail.activity as a, i (i)}
          <div class="line"><span class="time">{when(a.ts)}</span><span class="grow"><b class="by">{guardName(a.by)}</b> {a.text}</span></div>
        {/each}
      </div>
    {/if}
  </aside>
{/if}

<style>
  /* Starts below the window's own buttons (minimize, maximize, close) where Hub draws its title
     bar, like the other right panels' caption strip. */
  .detail { width: var(--panel-w); flex: none; border-left: 1px solid var(--line); background: var(--bg-sidebar); overflow-y: auto; padding: calc(var(--controls-h) + 10px) 20px 40px; font-size: 13px; }
  .head { display: flex; align-items: center; gap: 8px; color: var(--muted); font-size: 12px; }
  .headtext { min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .close { margin-left: auto; }
  h3 { color: var(--text); font-size: 15px; font-weight: 600; margin: 8px 0 3px; overflow-wrap: anywhere; }
  .where-line { color: var(--text-3); font-size: 12px; overflow-wrap: anywhere; }
  .mono, pre { font-family: var(--mono); }
  .say { color: var(--text-3); font-size: 13px; }
  .label { color: var(--muted); font-size: 12px; margin: 16px 0 6px; }
  .icon { display: grid; place-items: center; width: 26px; height: 26px; border-radius: var(--r-sm); color: var(--muted); }
  .icon:hover { color: var(--text); background: var(--surface-hover); }
  .lvl { width: 8px; height: 8px; border-radius: 50%; background: var(--muted); display: inline-block; flex: none; }
  .lvl.error { background: var(--red); }
  .lvl.warning { background: var(--amber); }
  .prio { font-family: var(--mono); font-size: 11px; padding: 0 4px; border: 1px solid var(--line-strong); border-radius: var(--r-sm); color: var(--text-3); }
  .prio.P0 { color: var(--red); border-color: var(--red); }
  .prio.P1 { color: var(--amber); border-color: var(--amber); }
  .vakten { background: var(--surface); border: 1px solid var(--line); border-radius: var(--r-md); padding: 12px 14px; }
  .vakten p { margin: 6px 0; color: var(--text-2); line-height: 1.45; }
  .vakten b { color: var(--text); font-weight: 600; }
  .vhead { color: var(--muted); font-size: 12px; display: flex; gap: 4px; }
  .who { color: var(--accent); }
  .chips { display: flex; flex-wrap: wrap; gap: 4px; margin-top: 10px; align-items: center; }
  .chip { font-size: 11px; padding: 1px 8px; border: 1px solid var(--line-strong); border-radius: var(--r-sm); color: var(--text-3); text-decoration: none; }
  .chip.warn { color: var(--amber); border-color: var(--amber); }
  .chip.pr:hover { color: var(--text-2); }
  .seg-row { display: flex; flex-wrap: wrap; gap: 8px; margin-top: 10px; }
  .seg { display: inline-flex; border: 1px solid var(--line-strong); border-radius: var(--r-md); overflow: hidden; }
  .seg button { padding: 3px 8px; font-size: 12px; color: var(--text-3); border-radius: 0; }
  .seg button:hover { background: var(--surface-hover); }
  .seg button.on { background: var(--accent-bg); color: var(--accent); }
  .acts { display: flex; flex-wrap: wrap; gap: 6px; margin-top: 12px; }
  .btn { height: 26px; padding: 0 10px; font-size: 13px; display: inline-flex; align-items: center; gap: 4px; }
  .btn.small { height: 22px; font-size: 12px; padding: 0 8px; flex: none; }
  .btn.primary { background: var(--accent-bg); color: var(--accent); }
  .btn.ghost { color: var(--muted); }
  .menuwrap { position: relative; }
  .menu { position: absolute; z-index: 5; top: 30px; left: 0; min-width: 260px; background: var(--bg-sidebar); border: 1px solid var(--line-strong); border-radius: var(--r-md); padding: 4px; box-shadow: 0 8px 24px rgb(0 0 0 / 0.4); }
  .menu button { display: flex; flex-direction: column; align-items: flex-start; width: 100%; text-align: left; padding: 6px 8px; border-radius: var(--r-sm); color: var(--text-2); font-size: 13px; }
  .menu button:hover { background: var(--surface-hover); }
  .menu small, .line small { color: var(--muted); font-size: 12px; }
  .list { display: flex; flex-direction: column; gap: 4px; }
  .line { display: flex; gap: 12px; align-items: baseline; color: var(--text-3); font-size: 13px; }
  .line .mono, .time { color: var(--muted); flex: none; min-width: 7ch; font-size: 12px; }
  .grow { flex: 1; min-width: 0; overflow-wrap: anywhere; }
  .by { color: var(--text-2); font-weight: 500; }
  .link { color: var(--accent); padding: 0; font-size: inherit; }
  .link.left { text-align: left; color: var(--text-2); }
  .merge { display: flex; gap: 6px; margin-top: 16px; }
  .field { height: 26px; padding: 0 8px; background: var(--bg); border: 1px solid var(--line-strong); border-radius: var(--r-md); font-size: 13px; outline: none; color: var(--text-2); min-width: 0; }
  .field.small { height: 22px; font-size: 12px; width: 13ch; }
  .field:focus { border-color: var(--accent); }
  pre { background: var(--surface); border: 1px solid var(--line); border-radius: var(--r-md); padding: 10px; font-size: 12px; color: var(--text-2); white-space: pre-wrap; overflow-wrap: anywhere; margin: 0 0 6px; }
  pre .lib { color: var(--muted); }
  pre .code { color: var(--text-3); }
  pre .crumb.warn, pre .crumb.warning { color: var(--amber); }
  pre .crumb.error { color: var(--red); }
  details summary { cursor: pointer; }
  .facts { display: flex; flex-wrap: wrap; align-items: center; gap: 4px; color: var(--text-3); font-size: 13px; }
  .facts :global(.glyph) { width: 11px; height: 11px; margin-right: 4px; }
  .sep { color: var(--muted); opacity: 0.7; }
  .tref { display: inline-flex; align-items: center; gap: 4px; padding: 0; color: var(--text-2); font-size: inherit; min-width: 0; }
  button.tref:hover { color: var(--text); text-decoration: underline; text-underline-offset: 3px; text-decoration-color: var(--line-strong); }
  pre.msg { margin-top: 8px; color: var(--text); }
  .label .more { float: right; text-transform: none; letter-spacing: 0; font-size: 12px; color: var(--muted); }
  .label .more:hover { color: var(--text); }
  .stack { display: flex; flex-direction: column; gap: 3px; font-family: var(--mono); font-size: 12px; border-left: 1px solid var(--line-strong); padding-left: 10px; }
  .frame { color: var(--muted); overflow-wrap: anywhere; }
  .frame.own .file { color: var(--text); }
  .frame.own .fn, .frame.own .ln { color: var(--text-3); }
  .frame.lib { font-style: italic; }
  .frame .code { color: var(--text-2); margin-top: 2px; padding-left: 12px; }
  .trail { display: flex; flex-direction: column; gap: 4px; font-size: 13px; }
  .crumb { display: grid; grid-template-columns: 7ch 8px 1fr; gap: 8px; align-items: baseline; color: var(--text-3); }
  .crumb .dot { align-self: start; margin-top: 4px; }
  .crumb .time { font-family: var(--mono); font-size: 12px; color: var(--muted); }
  .crumb .cat { color: var(--muted); margin-right: 6px; }
  .crumb.now { color: var(--red); }
  .sendoff { display: inline-flex; align-items: center; gap: 6px; font-size: 12px; color: var(--muted); animation: sendoff-in var(--dur-move) var(--ease); }
  .sendoff .dot { width: 7px; height: 7px; border-radius: 50%; border: 1.5px solid var(--muted); }
  .sendoff.sending .dot { animation: sendoff-pulse 0.9s var(--ease-move) infinite; }
  .sendoff.sent { color: var(--text); }
  .sendoff.sent .dot { border-color: var(--green); background: var(--green); animation: pulse-ok 0.2s var(--ease) 1; }
  @media (prefers-reduced-motion: reduce) { .sendoff.sent .dot { animation: none; } }
  @keyframes sendoff-in { from { opacity: 0; transform: translateX(-4px); } }
  @keyframes sendoff-pulse { 50% { transform: scale(0.6); opacity: 0.5; } }
  @media (prefers-reduced-motion: reduce) { .sendoff, .sendoff .dot { animation: none !important; } }
</style>
