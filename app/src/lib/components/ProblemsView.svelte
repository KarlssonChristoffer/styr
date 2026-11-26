<!-- Problems: every error and warning Hub caught, from its core, the app window, threads that failed, quiet sources, slower releases and the user's own programs, on this machine and the others that are online. The same error is one row with a count. Guard sorts them (prio, lane, category) and fixes what its lane allows; the tabs follow that work: what needs you, what Guard is on, what was fixed, what came back. The search takes Sentry-style terms; the dropdowns beside it filter by machine, project, source and prio, every column sorts, and problems with the same title in several places fold into one row. A row that waits for you says what it asks in words, with the button that answers it.
     Used by App.svelte. -->
<script lang="ts">
  import { autofocus } from "../autofocus";
  import { quiet } from "../quiet";
  import Quiet from "./Quiet.svelte";
  import { onMount, type ComponentProps } from "svelte";
  import { api } from "../api";
  import type { Level } from "../bindings/Level";
  import type { MetricSummary } from "../bindings/MetricSummary";
  import type { Problem } from "../bindings/Problem";
  import type { SavedView } from "../bindings/SavedView";
  import type { ToolRate } from "../bindings/ToolRate";
  import type { ToolReview } from "../bindings/ToolReview";
  import type { ToolFinding } from "../bindings/ToolFinding";
  import type { View } from "../bindings/View";
  import type { VaktenStatus } from "../bindings/VaktenStatus";
  import { ago, clock, statusLabel } from "../format";
  import { SOURCES } from "../problems";
  import type { StatusShadow } from "../bindings/StatusShadow";
  import { hubdHas } from "../compat";
  import { hub } from "../state.svelte";
  import { guardName } from "../system";
  import Icon from "./Icon.svelte";
  import ProviderMark from "./ProviderMark.svelte";
  import Ref from "./Ref.svelte";
  import { machineGlyph } from "../refs";
  import { engineProvider } from "../providers.svelte";
  import { openUrl } from "../native";
  import GuardPanel from "./GuardPanel.svelte";
  import ProblemDetail from "./ProblemDetail.svelte";
  import ProblemsSetup from "./ProblemsSetup.svelte";
  import Select from "./Select.svelte";
  import { fixState, startFix } from "../fixes.svelte";
  import { type Row, type ToolRow, type Group, type Sort, DAY, TABS, inView, pct, toolPeak, CAUSES, PRIO_RANK, sum, sortBy, RING_TIP, SOURCE_ICON, when, peak, fmt, SIGN_IN } from "../problemsView";


  let view = $state<View>("needs_you");
  let level = $state<Level>("warning");
  let query = $state("");
  let rows = $state<Row[]>([]);
  let failed = $state<string | null>(null);
  let picked = $state<{ id: number; host: string | null } | null>(null);
  let panel = $state<"" | "vakten" | "rules" | "connect">("");
  let vakten = $state<VaktenStatus | null>(null);
  let saved = $state<SavedView[]>([]);
  let naming = $state(false);
  let newView = $state("");
  let metrics = $state<MetricSummary[]>([]);
  let chosen = $state(false);
  // The Tool calls tab: every agent tool call counted per engine and tool, on each machine.
  let tools = $state(false);
  let toolDays = $state("7");
  let toolRows = $state<ToolRow[]>([]);
  let toolTotals = $state({ calls: 0, failed: 0 });
  let toolReview = $state<ToolReview | null>(null);

  // Filters and sorting, kept between visits.
  const kept = (key: string, fallback: string) => localStorage.getItem(`problems.${key}`) ?? fallback;
  let machine = $state(kept("machine", ""));
  let project = $state(kept("project", ""));
  let source = $state(kept("source", ""));
  let prio = $state(kept("prio", ""));
  let fold = $state(kept("fold", "1") === "1");
  let sort = $state<Sort>(JSON.parse(kept("sort", '{"by":"last","up":false}')));
  let opened = $state<Record<string, boolean>>({});
  let toolText = $state("");
  let toolMachine = $state(kept("toolMachine", ""));
  let toolEngine = $state(kept("toolEngine", ""));
  let toolShow = $state(kept("toolShow", "failing"));
  let toolSort = $state<Sort>(JSON.parse(kept("toolSort", '{"by":"failed","up":false}')));
  $effect(() => {
    const keep: Record<string, string> = {
      machine, project, source, prio, fold: fold ? "1" : "0", sort: JSON.stringify(sort),
      toolMachine, toolEngine, toolShow, toolSort: JSON.stringify(toolSort),
    };
    for (const [k, v] of Object.entries(keep)) localStorage.setItem(`problems.${k}`, v);
  });



  // This machine, then every other one that is online. Problems stay where they happened.
  const others = $derived(hub.hosts.filter((h) => !h.local && h.state === "online").map((h) => h.name));

  async function load() {
    const filter = { view: "all" as View, level, query: query.trim() || undefined, limit: 2000 };
    const asks: Promise<Row[]>[] = [api.problems(filter).then((ps) => ps.map((p) => ({ ...p, host: null })))];
    for (const host of others) {
      asks.push(quiet(api.problems(filter, host).then((ps) => ps.map((p) => ({ ...p, host }))), "problems", []));
    }
    try {
      const all = (await Promise.all(asks)).flat();
      // The sidebar's count reads the same machines; read it again now so both say the same.
      void hub.refreshProblemsNeedYou();
      rows = all.sort((a, b) => b.last_seen - a.last_seen);
      failed = null;
      // The first time, open on the tab that has something in it.
      if (!chosen && !rows.some((p) => p.needs_you)) view = rows.some((p) => inView(p, "vakten")) ? "vakten" : "open";
      chosen = true;
    } catch (e) {
      failed = e instanceof Error ? e.message : String(e);
    }
  }

  function loadVakten() {
    quiet(api.vakten().then((v) => (vakten = v)), "vakten");
  }

  // Shadow mode of the thread status: shown only when the events and the thread have disagreed.
  let shadow = $state<StatusShadow | null>(null);
  async function loadShadow() {
    shadow = (await hubdHas("status.shadow")) ? await quiet(api.statusShadow(), "status shadow", null) : null; // compat-until: 2026-12-06
  }
  async function clearShadow() {
    shadow = await quiet(api.statusShadowClear(), "clear status shadow", shadow);
  }

  onMount(() => {
    void loadShadow();
    hub.problemsNew = [];
    if (hub.problemPick) {
      picked = hub.problemPick;
      hub.problemPick = null;
    }
    quiet(api.problemViews().then((v) => (saved = v)), "problem views");
  });
  // Again when the search changes or a problem comes in, a moment later so a burst is one read.
  $effect(() => {
    void level;
    void query;
    void others.length;
    void hub.problemsTick;
    const timer = setTimeout(load, hub.problemsTick ? 400 : 250);
    return () => clearTimeout(timer);
  });
  $effect(() => {
    void hub.problemsTick;
    if (hub.view === "problems") hub.problemsNew = [];
    const t = setTimeout(loadVakten, 800);
    return () => clearTimeout(t);
  });
  async function loadTools() {
    const days = Number(toolDays);
    const hosts: (string | null)[] = [null, ...others];
    const asks = hosts.map((host) => api.toolStats(days, host).then((s) => ({ s, host }), () => null));
    const got = (await Promise.all(asks)).filter((g) => g != null);
    toolRows = got.flatMap(({ s, host }) => s.tools.map((t) => ({ ...t, host }))).sort((a, b) => b.failed - a.failed || b.calls - a.calls);
    toolTotals = { calls: got.reduce((n, g) => n + g.s.calls, 0), failed: got.reduce((n, g) => n + g.s.failed, 0) };
    // One Guard looks after every machine: the newest review wherever it is kept.
    const reviews = await Promise.all(hosts.map((host) => quiet(api.toolReview(host), "tool review", null)));
    toolReview = reviews.filter((r) => r != null).sort((a, b) => b.ts - a.ts)[0] ?? null;
  }
  $effect(() => {
    void toolDays;
    void hub.problemsTick;
    if (!tools) return;
    const t = setTimeout(() => loadTools().catch((e) => (failed = e instanceof Error ? e.message : String(e))), 300);
    return () => clearTimeout(t);
  });
  /** Guard's finding on a row, by engine, tool and machine (none in the finding is this machine). */
  const findingFor = (t: ToolRow): ToolFinding | undefined =>
    toolReview?.findings.find((f) => f.engine === t.engine && f.tool === t.tool && (f.machine || hub.localHost) === (t.host ?? hub.localHost));
  $effect(() => {
    if (view === "slower") quiet(api.problemMetrics().then((m) => (metrics = m)), "problem metrics");
  });

  const counts = $derived(Object.fromEntries(TABS.map(([v]) => [v, rows.filter((p) => inView(p, v)).length])) as Record<View, number>);
  const where = (p: Row) => p.host ?? p.machine;
  const inTab = $derived(rows.filter((p) => inView(p, view)));
  const shown = $derived(
    inTab.filter(
      (p) =>
        (!machine || where(p) === machine) &&
        (!project || p.project === project) &&
        (!source || p.source === source) &&
        (!prio || (prio === "none" ? !p.prio : p.prio != null && p.prio <= prio)),
    ),
  );
  /** Each filter's choices with how many rows in this tab have them. */
  function choices(of: (p: Row) => string, all: string, name: (v: string) => string = (v) => v) {
    const n = new Map<string, number>();
    for (const p of inTab) n.set(of(p), (n.get(of(p)) ?? 0) + 1);
    return [
      { value: "", label: all },
      ...[...n].sort((a, b) => b[1] - a[1]).map(([v, c]) => ({ value: v, label: name(v), hint: String(c) })),
    ];
  }
  const filtering = $derived(!!(machine || project || source || prio));
  function clearFilters() {
    machine = project = source = prio = "";
  }

  // Guard's column, as a rank to sort by: what needs you first, what is done last.
  function rank(p: Row): number {
    const [tone] = vaktenSays(p);
    if (p.needs_you) return 0;
    return tone === "bad" ? 1 : tone === "work" ? 2 : p.status === "open" ? 3 : p.status === "archived" ? 4 : 5;
  }
  const KEYS: Record<string, (g: Group) => number | string> = {
    prio: (g) => -Math.min(...g.rows.map(PRIO_RANK)),
    problem: (g) => g.first.title.toLowerCase(),
    vakten: (g) => -Math.min(...g.rows.map(rank)),
    days: (g) => sum(g.days),
    hits: (g) => g.count,
    last: (g) => g.last,
  };
  const groups = $derived.by(() => {
    const by = new Map<string, Row[]>();
    for (const p of shown) {
      const key = fold ? `${p.source}\u0000${p.level}\u0000${p.title}` : `${p.host ?? ""}:${p.id}`;
      (by.get(key) ?? by.set(key, []).get(key)!).push(p);
    }
    const out: Group[] = [...by].map(([key, rs]) => {
      rs.sort((a, b) => b.last_seen - a.last_seen);
      return {
        key,
        rows: rs,
        first: rs.find((r) => r.needs_you) ?? rs[0],
        count: rs.reduce((n, r) => n + r.count, 0),
        last: rs[0].last_seen,
        days: rs[0].days.map((_, i) => rs.reduce((n, r) => n + (r.days[i] ?? 0), 0)),
      };
    });
    const key = KEYS[sort.by] ?? KEYS.last;
    const dir = sort.up ? 1 : -1;
    return out.sort((a, b) => {
      const [x, y] = [key(a), key(b)];
      return (x < y ? -1 : x > y ? 1 : 0) * dir || b.last - a.last;
    });
  });

  /** What a problem that waits for you asks, in words, and the button that answers it. */
  type Ask = { text: string; act?: { label: string; tip: string; run: (from?: Element) => void } };
  function asks(p: Row): Ask | null {
    const place = where(p);
    if (p.thread) {
      const open = () => (hub.threads.some((t) => t.id === p.thread) ? hub.open(p.thread!) : pick(p));
      if (p.needs_you) return { text: p.note?.next || "A thread is on it and waits for you", act: { label: "Open thread", tip: "The thread working on it", run: open } };
      return null;
    }
    if (p.lane === "ask" && SIGN_IN.test(p.title)) {
      const engine = /^([\w.-]+) failed:/.exec(p.title)?.[1] ?? "claude";
      return {
        text: `Sign in to ${engine} again on ${place}; no agent can`,
        act: { label: "Sign in", tip: `Opens a terminal on ${place} that signs ${engine} in for a year`, run: () => signIn(place, engine) },
      };
    }
    const start = (plan: string | null, label: string, tip: string) => ({ label, tip, run: (from?: Element) => startFix(p.id, plan, p.host, from) });
    if (p.lane === "ask" && p.note?.plan)
      return { text: `Guard has a plan and asks first: ${p.note.plan}`, act: start(p.note.plan, "Let Guard do it", "Starts a thread with Guard's plan; it ends at a pull request") };
    if (p.lane === "ask") return { text: p.note?.next || p.note?.what || "Asks before it changes anything: decide what to do", act: start(null, "Start thread", "A background thread fixes it on a small model and never asks you; you stay here") };
    if (p.regressed) return { text: "Came back after it was fixed: look at it again", act: start(null, "Start thread", "A background thread fixes it on a small model and never asks you; you stay here") };
    if (p.prio === "P0" || p.prio === "P1") return { text: "Urgent and nobody is on it", act: start(null, "Start thread", "A background thread fixes it on a small model and never asks you; you stay here") };
    if (p.needs_you) return { text: vakten?.prefs.on ? "Waits for Guard to sort it" : "Guard is off: start a thread or archive it", act: start(null, "Start thread", "A background thread fixes it on a small model and never asks you; you stay here") };
    return null;
  }
  async function signIn(place: string, engine: string) {
    try {
      const r = await api.openOnMachine(place, { what: "sign_in", engine });
      if (!r.opened) hub.tell(`Run this in a terminal: ${r.attach}`);
    } catch (e) {
      hub.fail(e);
    }
  }

  // The Tool calls tab's filters and sorting.
  const toolsShown = $derived.by(() => {
    const text = toolText.trim().toLowerCase();
    const list = toolRows.filter(
      (t) =>
        (toolShow === "all" || t.failed > 0) &&
        (!toolMachine || (t.host ?? hub.localHost) === toolMachine) &&
        (!toolEngine || t.engine === toolEngine) &&
        (!text || t.tool.toLowerCase().includes(text) || t.errors.some((e) => e.gist.toLowerCase().includes(text))),
    );
    const key: Record<string, (t: ToolRow) => number | string> = {
      tool: (t) => t.tool.toLowerCase(),
      calls: (t) => t.calls,
      failed: (t) => t.failed,
      rate: (t) => (t.calls ? t.failed / t.calls : 0),
      days: (t) => t.days.findLastIndex((d) => d > 0),
    };
    const k = key[toolSort.by] ?? key.failed;
    const dir = toolSort.up ? 1 : -1;
    return list.sort((a, b) => {
      const [x, y] = [k(a), k(b)];
      return (x < y ? -1 : x > y ? 1 : 0) * dir || b.failed - a.failed;
    });
  });
  const toolChoices = (of: (t: ToolRow) => string, all: string) => {
    const n = new Map<string, number>();
    for (const t of toolRows) n.set(of(t), (n.get(of(t)) ?? 0) + t.calls);
    return [{ value: "", label: all }, ...[...n].sort((a, b) => b[1] - a[1]).map(([v, c]) => ({ value: v, label: v, hint: `${c} calls` }))];
  };
  const worst = $derived([...toolRows].filter((t) => t.failed >= 3).sort((a, b) => b.failed / b.calls - a.failed / a.calls)[0]);
  const fixedToday = $derived(rows.filter((p) => p.status === "resolved" && (p.resolved_at ?? 0) >= Date.now() - DAY && p.resolved_by !== "you").length);

  function vaktenSays(p: Row): [string, string] {
    if (p.status === "resolved") return ["ok", p.pr?.state === "merged" ? `fixed · PR #${p.pr.number} merged` : `fixed · by ${p.resolved_by ?? "you"}`];
    if (p.status === "archived") return ["", "archived"];
    if (p.regressed) return ["bad", "came back"];
    if (p.lane === "ask" && !p.thread) return ["bad", p.note?.plan ? "asks you · plan ready" : "asks you"];
    if (p.thread) return ["work", p.pr ? `fixing · PR #${p.pr.number}` : "fixing · thread"];
    if ((p.prio === "P0" || p.prio === "P1") && !p.thread) return ["bad", "urgent · nobody on it"];
    if (p.lane === "quiet") return ["", "quiet · counted"];
    if (p.stamped_by && (p.lane || p.stamped_by !== "rule")) return ["", `sorted by ${guardName(p.stamped_by)}`];
    // One Guard, this machine's, reads every machine's problems.
    const guard = vakten;
    if (p.stamped_by === "rule" && !p.lane) return ["", guard?.prefs.on ? "Hub's guess · in Guard's queue" : "Hub's guess"];
    return ["", guard?.prefs.on ? "in Guard's queue" : "not sorted"];
  }

  // Hub's visual language: the row's ring says its state on the same scale as threads (amber
  // ring waits for you, pulsing green is being worked on, red fill went wrong again, lavender fill
  // is done, grey ring is left as it is); grey glyphs say what the rest is.
  function ring(p: Row): string {
    if (p.status === "resolved") return "idle unread";
    if (p.status === "archived") return "idle";
    if (p.needs_you) return p.regressed ? "error" : "waiting_on_you";
    const [tone] = vaktenSays(p);
    return tone === "work" ? "running" : tone === "bad" ? "error" : "idle";
  }
  const hostOf = (name: string) => hub.hostInfo(name);
  const threadOf = (p: Row) => (p.thread ? hub.threads.find((t) => t.id === p.thread) : undefined);

  function pick(p: Row) {
    picked = { id: p.id, host: p.host };
  }

  function useView(s: SavedView) {
    query = s.query;
    if (s.view) view = s.view;
  }

  async function saveView() {
    const name = newView.trim();
    if (!name) return;
    try {
      saved = await api.setProblemViews([...saved.filter((s) => s.name !== name), { name, query, view }]);
      naming = false;
      newView = "";
    } catch (e) {
      hub.fail(e);
    }
  }

  async function dropView(s: SavedView) {
    try {
      saved = await api.setProblemViews(saved.filter((x) => x.name !== s.name));
    } catch (e) {
      hub.fail(e);
    }
  }

  // Every count here is problems, not rows: Fold alike can show several problems as one row, and
  // the line under the list says how many rows they took. The sidebar counts the same set, over
  // every level; the tab follows the level and search picked here.
  const needYou = $derived(counts.needs_you ?? 0);
  const summary = $derived.by(() => {
    const open = counts.open ?? 0;
    if (!vakten?.prefs.on) return `${open} open, ${needYou} wait for you. Guard is off; turn it on to have them sorted and fixed.`;
    return `Guard sorts ${open} open: fixed ${fixedToday} by itself today, working on ${counts.vakten ?? 0}, ${needYou} wait for you.`;
  });
  /** A line of what stands out: what came in today, where most is, where it comes from. */
  const facts = $derived.by(() => {
    const open = rows.filter((p) => inView(p, "open"));
    if (!open.length) return "";
    const today = open.filter((p) => p.first_seen >= Date.now() - DAY).length;
    const hits = open.reduce((n, p) => n + (p.days[6] ?? 0), 0);
    const top = (of: (p: Row) => string) => {
      const n = new Map<string, number>();
      for (const p of open) n.set(of(p), (n.get(of(p)) ?? 0) + 1);
      return [...n].sort((a, b) => b[1] - a[1])[0];
    };
    const [m, mn] = top(where);
    const [src, sn] = top((p) => p.source);
    const parts = [`Today ${today} new, ${hits} hits`, `most open on ${m} (${mn})`, `most from ${SOURCES[src] ?? src} (${sn})`];
    const escalating = open.filter((p) => p.escalating).length;
    if (escalating) parts.push(`${escalating} escalating`);
    return parts.join(" · ") + ".";
  });
</script>

<div class="page">
  <div class="list-side">
    <header>
      <h2>Problems</h2>
      <span class="tools">
        {#each [["vakten", "Guard"], ["rules", "Rules"], ["connect", "Connect a program"]] as [k, label] (k)}
          <button class="btn ghost" class:on={panel === k} onclick={() => (panel = panel === k ? "" : (k as typeof panel))}>
            {#if k === "vakten"}<span class="vdot {vakten?.state ?? 'off'}"></span>{/if}{label}
          </button>
        {/each}
      </span>
    </header>
    <p class="summary">{summary}</p>
    {#if facts && !tools}<p class="facts">{facts}</p>{/if}

    {#if shadow && shadow.disagreed > 0}
      <div class="review shadow">
        <div class="rhead">
          Status disagreed {shadow.disagreed} of {shadow.checked} times
          <button class="clear" onclick={clearShadow}>Clear</button>
        </div>
        {#each shadow.recent.slice(0, 3) as m (m.thread_id + m.seq)}
          <p>{m.title || "Untitled"} · after {m.kind.replaceAll("_", " ")}: events say {statusLabel[m.shadow]}, thread was {statusLabel[m.real]} · {when(m.at)}</p>
        {/each}
      </div>
    {/if}

    {#if panel}{#key panel}<ProblemsSetup which={panel} bind:vakten />{/key}{/if}

    <nav class="tabs">
      {#each TABS as [v, label] (v)}
        {#if counts[v] || v === "needs_you" || v === "open" || view === v}
          <button class:on={view === v && !tools} data-tip={v === "needs_you" ? "Waiting for you on every online machine" : undefined} onclick={() => ((view = v), (tools = false))}>{label} <span class="n" class:red={v === "needs_you" && counts[v]}>{counts[v] ?? 0}</span></button>
        {/if}
      {/each}
      <button class:on={tools} onclick={() => (tools = true)} data-tip="Calls per engine and tool">Tool calls</button>
      {#each saved as s (s.name)}
        <span class="saved">
          <button class:on={query === s.query && (!s.view || view === s.view)} onclick={() => useView(s)}>{s.name}</button>
          <button class="x" aria-label="Forget this view" onclick={() => dropView(s)}><Icon name="x" size={10} /></button>
        </span>
      {/each}
      {#if naming}
        <form class="name" onsubmit={(e) => (e.preventDefault(), saveView())}>
          <input class="field" placeholder="Name this view" bind:value={newView} use:autofocus onblur={() => !newView && (naming = false)} />
        </form>
      {:else}
        <button class="plus" onclick={() => (naming = true)} data-tip="Keep this tab and search as a view of its own">+ Save view</button>
      {/if}
    </nav>

    {#if !tools}
      <div class="filters">
        <div class="search">
          <Icon name="search" size={13} />
          <input
            class="q"
            placeholder="is:new level:error release:0.52 lane:ask times_seen:>10 lastSeen:-24h"
            bind:value={query}
            spellcheck="false"
          />
        </div>
        <span class="pick" class:set={!!machine}><Select size="small" label="Machine" bind:value={machine} options={choices(where, "All machines")} /></span>
        <span class="pick" class:set={!!project}><Select size="small" label="Project" bind:value={project} options={choices((p) => p.project, "All projects")} /></span>
        <span class="pick" class:set={!!source}><Select size="small" label="Source" bind:value={source} options={choices((p) => p.source, "All sources", (v) => SOURCES[v] ?? v)} /></span>
        <span class="pick" class:set={!!prio}><Select size="small" label="Prio" bind:value={prio}
          options={[{ value: "", label: "Any prio" }, { value: "P0", label: "P0 only" }, { value: "P1", label: "P1 and up" }, { value: "P2", label: "P2 and up" }, { value: "none", label: "No prio yet" }]} /></span>
        <span class="pick"><Select size="small" label="Level" bind:value={level}
          options={[{ value: "error", label: "Errors" }, { value: "warning", label: "Warnings and errors" }, { value: "info", label: "Everything" }]} /></span>
        <button class="toggle" class:on={fold} onclick={() => (fold = !fold)} data-tip="Same title, one row">Fold alike</button>
        {#if filtering}<button class="clear" onclick={clearFilters}>Clear</button>{/if}
      </div>
    {/if}

    {#if view === "slower" && !tools && metrics.length}
      <div class="metrics">
        {#each metrics as m (m.name)}
          {@const worse = m.previous_median != null && m.median > m.previous_median * 1.5}
          <div class="metric">
            <span class="mname">{m.label}{m.name.includes("/") ? ` · ${m.name.split("/")[0]}` : ""}</span>
            <span class="mval" class:bad={worse}>{fmt(m, m.median)} in {m.release}</span>
            <span class="mprev">{m.previous_median != null ? `${fmt(m, m.previous_median)} in ${m.previous_release}` : "no earlier release yet"}</span>
          </div>
        {/each}
      </div>
    {/if}

    {#if failed}<p class="say bad">{failed}</p>{/if}
    {#if tools}
      <div class="tool-head">
        <p class="say">
          {toolTotals.calls} tool calls by agents in {toolDays} days, {toolTotals.failed} failed ({pct(toolTotals.failed, toolTotals.calls)}).
          {#if worst}Fails most often: <span class="mono">{worst.tool}</span> on {worst.engine}, {worst.failed} of {worst.calls}.{/if}
        </p>
      </div>
      {#if toolReview}
        <div class="review">
          <div class="rhead"><span class="vdot idle"></span>Guard's review · {when(toolReview.ts)}</div>
          <p>{toolReview.text}</p>
        </div>
      {/if}
      <div class="filters">
        <div class="search">
          <Icon name="search" size={13} />
          <input class="q" placeholder="A tool, or words in what failed" bind:value={toolText} spellcheck="false" />
        </div>
        <span class="pick" class:set={!!toolMachine}><Select size="small" label="Machine" bind:value={toolMachine} options={toolChoices((t) => t.host ?? hub.localHost, "All machines")} /></span>
        <span class="pick" class:set={!!toolEngine}><Select size="small" label="Engine" bind:value={toolEngine} options={toolChoices((t) => t.engine, "All engines")} /></span>
        <span class="pick"><Select size="small" label="Show" bind:value={toolShow} options={[{ value: "failing", label: "Tools that failed" }, { value: "all", label: "Every tool" }]} /></span>
        <span class="pick"><Select size="small" label="Days" bind:value={toolDays} options={[{ value: "7", label: "7 days" }, { value: "30", label: "30 days" }, { value: "90", label: "90 days" }]} /></span>
        {#if toolMachine || toolEngine || toolText}<button class="clear" onclick={() => (toolMachine = toolEngine = toolText = "")}>Clear</button>{/if}
      </div>
      {#if !toolRows.length}
        <Quiet text="No tool calls counted" />
      {:else if !toolsShown.length}
        <Quiet text="No tool matches these filters" />
      {:else}
        <div class="rows">
          <div class="trow th">
            {#each [["tool", "Tool", ""], ["calls", "Calls", "r"], ["failed", "Failed", "r"], ["rate", "Rate", "r"], ["days", "Failures", ""]] as [k, label, cls] (k)}
              <button class="sorter {cls}" class:on={toolSort.by === k} onclick={() => (toolSort = sortBy(k, toolSort))}>{label}{#if toolSort.by === k}<span class="arrow" class:up={toolSort.up}><Icon name="chevron" size={10} /></span>{/if}</button>
            {/each}
            <span>What failed, most often first</span>
          </div>
          {#each toolsShown as t (`${t.host ?? ""}:${t.engine}:${t.tool}`)}
            {@const rate = t.calls ? t.failed / t.calls : 0}
            {@const m = t.host ?? hub.localHost}
            <div class="trow">
              <span class="main">
                <span class="title mono">{t.tool}</span>
                <span class="meta">
                  <span class="src"><ProviderMark id={engineProvider(t.engine).id} size={11} />{t.engine}</span>
                  <span class="sep">·</span>
                  <Ref kind={machineGlyph(m, !t.host)} label={m || "this machine"} online={hostOf(m)?.state === "online"} />
                </span>
                {#if findingFor(t)}
                  {@const f = findingFor(t)!}
                  <span class="finding" data-tip={f.action}><span class="cause" class:amber={f.verdict === "waits"}>{CAUSES[f.cause] ?? f.cause}</span> · {f.action}</span>
                {/if}
              </span>
              <span class="count">{t.calls}</span>
              <span class="count" class:warn={t.failed > 0}>{t.failed}</span>
              <span class="count" class:warn={rate >= 0.1} class:bad={rate >= 0.25 && t.failed >= 3}>{pct(t.failed, t.calls)}</span>
              <span class="spark" data-tip="Failures per day">
                {#each t.days.slice(-14) as d, i (i)}<i class:hot={d > 0} style="height: {d ? 3 + (d / toolPeak(t)) * 13 : 2}px"></i>{/each}
              </span>
              <span class="tops">
                {#each t.errors.slice(0, 3) as e (e.gist)}
                  {#if e.problem != null}
                    <button class="top" onclick={() => (picked = { id: e.problem!, host: t.host })} data-tip={e.gist}><span class="n">{e.count}×</span> {e.gist}</button>
                  {:else}
                    <span class="top" data-tip="{e.gist}"><span class="n">{e.count}×</span> {e.gist}</span>
                  {/if}
                {:else}
                  {#if t.failed}<span class="top muted">Not known what it said</span>{/if}
                {/each}
                {#if t.errors.length > 3}<span class="more">and {t.errors.length - 3} more kinds</span>{/if}
              </span>
            </div>
          {/each}
        </div>
      {/if}
    {:else if !shown.length && !failed}
      <p class="empty">
        {view === "needs_you" ? "Nothing needs you." : view === "open" ? "Nothing open." : "Nothing here."}
      </p>
    {:else}
      <div class="rows">
        <div class="row th">
          {#each [["prio", "Prio", ""], ["", "", ""], ["problem", "Problem", ""], ["vakten", "Guard", "says-h"], ["", "", "act"], ["days", "7 days", "spark"], ["hits", "Hits", "r"], ["last", "Last", ""]] as [k, label, cls], i (i)}
            {#if k}
              <button class="sorter {cls}" class:on={sort.by === k} onclick={() => (sort = sortBy(k, sort))}>{label}{#if sort.by === k}<span class="arrow" class:up={sort.up}><Icon name="chevron" size={10} /></span>{/if}</button>
            {:else}<span class={cls}></span>{/if}
          {/each}
        </div>
        {#each groups as g (g.key)}
          {@const many = g.rows.length > 1}
          {@const open = many && opened[g.key]}
          {#each open ? [g.first, ...g.rows] : [g.first] as p, i (i === 0 ? "top" : `${p.host ?? ""}:${p.id}`)}
            {@const [tone, says] = vaktenSays(p)}
            {@const ask = asks(p)}
            {@const head = i === 0}
            {@const folded = head && many}
            <div class="row" class:sel={!folded && picked?.id === p.id && picked?.host === p.host} class:child={!head}
              role="button" tabindex="0"
              onclick={() => (folded ? (opened[g.key] = !opened[g.key]) : pick(p))}
              onkeydown={(e) => e.key === "Enter" && (folded ? (opened[g.key] = !opened[g.key]) : pick(p))}>
              <span class="prio {p.prio ?? ''}">{p.prio ?? "–"}</span>
              <span class="dot {folded ? ring(g.rows.find((r) => r.needs_you) ?? p) : ring(p)}" data-tip={RING_TIP[ring(p)]} aria-label={RING_TIP[ring(p)]}></span>
              <span class="main">
                <span class="title">
                  {#if folded}<span class="fold" class:open><Icon name="chevron" size={11} /></span>{/if}
                  {head ? p.title : p.project}
                </span>
                <span class="meta">
                  {#if folded}
                    <span>{g.rows.length} places</span>
                    <span class="sep">·</span>
                    <span class="src"><Icon name={SOURCE_ICON[p.source] ?? "alert"} size={11} />{SOURCES[p.source] ?? p.source}</span>
                    {#each [...new Set(g.rows.map(where))] as m (m)}
                      {@const h = hostOf(m)}
                      <span class="sep">·</span><Ref kind={machineGlyph(m, !!h?.local)} label={m} online={h?.state === "online"} />
                    {/each}
                  {:else}
                    {#if head}
                      {#if p.folder}<Ref kind="project" project={{ path: p.folder, host: p.host }} label={p.project} title={p.folder} />{:else}<Ref kind="project" label={p.project} />{/if}
                      <span class="sep">·</span>
                    {/if}
                    <span class="src"><Icon name={SOURCE_ICON[p.source] ?? "alert"} size={11} />{SOURCES[p.source] ?? p.source}</span>
                    <span class="sep">·</span>
                    <Ref kind={machineGlyph(where(p), !!hostOf(where(p))?.local)} label={where(p)} online={hostOf(where(p))?.state === "online"} />
                    {#if p.level === "warning"}<span class="sep">·</span><span>warning</span>{/if}
                    {#if p.category}<span class="sep">·</span><span class:red={p.category === "security"}>{p.category}</span>{/if}
                    {#if p.escalating}<span class="sep">·</span><span class="amber">escalating</span>{/if}
                    {#if p.regressed}<span class="sep">·</span><span class="red">came back</span>{/if}
                    {#if p.new}<span class="sep">·</span><span>new</span>{/if}
                  {/if}
                </span>
              </span>
              <span class="says">
                <span class="state" class:bad={tone === "bad" && !p.needs_you} class:asks={p.needs_you && !p.regressed} class:ok={tone === "ok"}>
                  {threadOf(p) && !folded ? "fixing" : says}
                  {#if threadOf(p) && !folded}
                    <span class="sep">·</span><span class="tref" role="link" tabindex="0" data-tip="Open the thread on it"
                      onclick={(e) => (e.stopPropagation(), hub.open(p.thread!))} onkeydown={(e) => e.key === "Enter" && hub.open(p.thread!)}>
                      <span class="dot {threadOf(p)!.status}"></span>{threadOf(p)!.title}</span>
                  {/if}
                  {#if p.pr && !folded}
                    <span class="sep">·</span><Ref kind="pr" tone={p.pr.state} label="#{p.pr.number}" title={p.pr.url} onopen={() => openUrl(p.pr!.url)} />
                  {/if}
                </span>
                {#if ask}<span class="ask" data-tip={ask.text}>{ask.text}</span>{/if}
              </span>
              <span class="act">
                {#if fixState(p.id, p.host) && !folded}
                  <span class="sendoff {fixState(p.id, p.host)}" data-tip="Fix thread">
                    <span class="dot"></span>{fixState(p.id, p.host) === "sending" ? "Sending" : "In Inbox, working"}</span>
                {:else if ask?.act && !folded}
                  <button class="btn small" class:primary={p.needs_you} data-tip={ask.act.tip} onclick={(e) => (e.stopPropagation(), ask.act!.run(e.currentTarget))}>{ask.act.label}</button>
                {:else if folded}
                  <span class="muted">{open ? "Fold" : "Show all"}</span>
                {/if}
              </span>
              <span class="spark" data-tip="The last 7 days">
                {#each head ? g.days : p.days as d, j (j)}<i class:hot={d > 0 && j === 6} style="height: {d ? 3 + (d / Math.max(1, ...(head ? g.days : p.days))) * 13 : 2}px"></i>{/each}
              </span>
              <span class="count">{head ? g.count : p.count}</span>
              <span class="seen">{when(head ? g.last : p.last_seen)}</span>
            </div>
          {/each}
        {/each}
      </div>
      {#if filtering || fold}
        <p class="tally">{shown.length} of {inTab.length} problems{fold && groups.length < shown.length ? `, in ${groups.length} rows` : ""}.</p>
      {/if}
    {/if}
  </div>

  {#if picked}
    <ProblemDetail id={picked.id} host={picked.host} onclose={() => (picked = null)} onopen={(id) => (picked = { id, host: picked?.host ?? null })} />
  {:else if vakten?.prefs.on || hub.guardThreads.length}
    <GuardPanel {vakten} onsettings={() => (panel = "vakten")} />
  {/if}
</div>

<style>
  .page { flex: 1; min-height: 0; display: flex; width: 100%; }
  .list-side { flex: 1; min-width: 0; container-type: inline-size; overflow-y: auto; padding: var(--page-top) var(--gutter) 60px; font-size: 13px; }
  header { display: flex; align-items: baseline; gap: 12px; }
  h2 { color: var(--text); font-size: var(--title); font-weight: 500; letter-spacing: -0.01em; margin: 0; }
  .tools { margin-left: auto; display: flex; gap: 2px; }
  .summary { color: var(--text-3); margin: 6px 0 0; max-width: var(--prose); }
  .say { color: var(--text-3); font-size: 13px; margin: 0 0 8px; }
  .say.bad { color: var(--red); }
  .empty { color: var(--muted); margin-top: 22px; }
  .btn { height: 26px; padding: 0 10px; font-size: 13px; display: inline-flex; align-items: center; gap: 6px; }
  .btn.ghost { color: var(--text-3); }
  .btn.ghost.on { color: var(--text); background: var(--surface); }
  .vdot { width: 7px; height: 7px; border-radius: 50%; background: var(--muted); }
  .vdot.working, .vdot.idle, .vdot.guarded { background: var(--green); }
  .vdot.waiting { background: var(--amber); }
  .vdot.paused { background: var(--amber); }

  .tabs { display: flex; flex-wrap: wrap; align-items: center; gap: 2px; margin: 16px 0 10px; }
  .tabs button { height: 26px; padding: 0 8px; border-radius: var(--r-md); color: var(--text-3); font-size: 13px; }
  .tabs button:hover { background: var(--surface-hover); }
  .tabs button.on { background: var(--accent-bg); color: var(--accent); }
  .n { color: var(--muted); margin-left: 3px; }
  .n.red { color: var(--red); }
  .on .n { color: inherit; }
  .saved { display: inline-flex; align-items: center; }
  .saved .x { width: 16px; padding: 0; opacity: 0; color: var(--muted); }
  .saved:hover .x, .saved:focus-within .x { opacity: 1; }
  .plus { color: var(--muted) !important; }
  .name .field { width: 18ch; }

  .facts { color: var(--muted); margin: 2px 0 0; font-size: 13px; }
  .filters { display: flex; align-items: center; gap: 6px; flex-wrap: wrap; margin-bottom: 12px; }
  .search { flex: 1 1 320px; display: flex; align-items: center; gap: 8px; padding: 0 10px; height: 30px; border: 1px solid var(--line-strong); border-radius: var(--r-md); color: var(--muted); }
  .pick { display: inline-flex; border-radius: var(--r-md); }
  .pick.set :global(button) { color: var(--accent); border-color: var(--accent); }
  .toggle, .clear { height: 26px; padding: 0 8px; border-radius: var(--r-md); font-size: 13px; color: var(--text-3); border: 1px solid var(--line-strong); }
  .toggle.on { color: var(--accent); border-color: var(--accent); background: var(--accent-bg); }
  .clear { border-color: transparent; color: var(--muted); }
  .clear:hover { color: var(--text); }
  .sorter { display: inline-flex; align-items: center; gap: 3px; padding: 0; height: auto; font: inherit; color: inherit; text-transform: inherit; letter-spacing: inherit; border-radius: 0; background: none; text-align: left; }
  .sorter.r { justify-content: flex-end; }
  .sorter:hover, .sorter.on { color: var(--text); }
  .arrow { display: inline-flex; transform: rotate(90deg); color: var(--accent); }
  .arrow.up { transform: rotate(-90deg); }
  .fold { display: inline-flex; color: var(--muted); margin-right: 4px; vertical-align: -1px; transition: transform 0.12s var(--ease); }
  .fold.open { transform: rotate(90deg); }
  .tally { color: var(--muted); font-size: 12px; margin: 8px 0 0; }
  .search:focus-within { border-color: var(--accent); }
  .q { flex: 1; min-width: 0; background: none; border: none; outline: none; font-family: var(--mono); font-size: 12px; color: var(--text-2); }
  .field { height: 26px; padding: 0 8px; background: var(--bg); border: 1px solid var(--line-strong); border-radius: var(--r-md); font-size: 13px; outline: none; color: var(--text-2); min-width: 0; }

  .metrics { display: grid; gap: 4px; margin-bottom: 12px; padding: 8px 10px; background: var(--surface); border-radius: var(--r-md); }
  .metric { display: grid; grid-template-columns: minmax(0, 1fr) 18ch 22ch; gap: 12px; font-size: 13px; color: var(--text-3); }
  .mname { color: var(--text-2); }
  .mval { font-family: var(--mono); }
  .mval.bad { color: var(--amber); }
  .mprev { color: var(--muted); font-family: var(--mono); }

  .rows { border-top: 1px solid var(--line); }
  .row { display: grid; grid-template-columns: 30px 10px minmax(0, 1fr) minmax(24ch, 44ch) 15ch 58px 5ch 9ch; align-items: center; gap: 12px; width: 100%; min-height: var(--row-min); padding: var(--row-pad) 8px; border-bottom: 1px solid var(--line); text-align: left; border-radius: 0; }
  .row { cursor: pointer; }
  .row:hover, .row.sel { background: var(--surface); }
  .row.child { background: color-mix(in srgb, var(--surface) 50%, transparent); }
  .row.child .main { padding-left: 16px; }
  .row.child .title { color: var(--text-2); font-size: 13px; }
  /* The header keeps the rows' font size, so its ch columns are as wide as theirs; only its
     cells are small. */
  .row.th, .trow.th { color: var(--muted); min-height: 0; padding: 6px 8px; }
  .row.th > *, .trow.th > * { font-size: 11px; }
  .sorter.r { justify-self: end; }
  .row.th:hover { background: none; }
  .row.th { cursor: default; }
  .r { text-align: right; }
  .prio { font-family: var(--mono); font-size: 11px; color: var(--muted); text-align: center; border: 1px solid var(--line); border-radius: var(--r-sm); line-height: 18px; }
  .prio.P0 { color: var(--red); border-color: var(--red); }
  .prio.P1 { color: var(--amber); border-color: var(--amber); }
  /* Only what is urgent gets a box; the rest is plain text. */
  .prio:not(.P0):not(.P1) { border-color: transparent; }
  .prio.P2, .prio.P3 { color: var(--text-3); }
  .main { display: flex; flex-direction: column; gap: var(--row-gap); min-width: 0; }
  .title { color: var(--text); overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .meta { color: var(--muted); font-size: 12px; display: flex; align-items: center; gap: 4px; min-width: 0; white-space: nowrap; overflow: hidden; }
  .says { display: flex; flex-direction: column; gap: 2px; min-width: 0; font-size: 12px; }
  .state { color: var(--text-3); display: flex; align-items: center; gap: 6px; white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
  .ask { color: var(--text-2); overflow: hidden; text-overflow: ellipsis; display: -webkit-box; -webkit-line-clamp: 2; line-clamp: 2; -webkit-box-orient: vertical; }
  .act { display: flex; justify-content: flex-start; }
  .act .muted { color: var(--muted); font-size: 12px; }
  .sendoff { display: inline-flex; align-items: center; gap: 6px; font-size: 12px; color: var(--muted); white-space: nowrap; animation: sendoff-in var(--dur-move) var(--ease); }
  .sendoff .dot { width: 7px; height: 7px; border-radius: 50%; border: 1.5px solid var(--muted); }
  .sendoff.sending .dot { animation: sendoff-pulse 0.9s var(--ease-move) infinite; }
  .sendoff.sent { color: var(--text); }
  .sendoff.sent .dot { border-color: var(--green); background: var(--green); animation: pulse-ok 0.2s var(--ease) 1; }
  @media (prefers-reduced-motion: reduce) { .sendoff.sent .dot { animation: none; } }
  @keyframes sendoff-in { from { opacity: 0; transform: translateX(-4px); } }
  @keyframes sendoff-pulse { 50% { transform: scale(0.6); opacity: 0.5; } }
  @media (prefers-reduced-motion: reduce) { .sendoff, .sendoff .dot { animation: none !important; } }
  .btn.small { height: 24px; padding: 0 8px; font-size: 12px; }
  .sep { opacity: 0.7; }
  .meta :global(.glyph) { width: 11px; height: 11px; margin-right: 4px; }
  .src { display: inline-flex; align-items: center; gap: 4px; }
  .src :global(svg:not(.mark)) { color: var(--muted); flex: none; }
  .meta .red, .state.bad { color: var(--red); }
  .meta .amber { color: var(--amber); }
  .state.asks { color: var(--amber); }
  .state.ok { color: var(--green); }
  .tref { display: inline-flex; align-items: center; gap: 4px; min-width: 0; overflow: hidden; text-overflow: ellipsis; color: var(--text-3); cursor: pointer; }
  .tref:hover { color: var(--text); text-decoration: underline; text-underline-offset: 3px; text-decoration-color: var(--line-strong); }
  .cause { color: var(--text-3); }
  .cause.amber { color: var(--amber); }
  .seen { color: var(--muted); font-size: 12px; white-space: nowrap; }
  .count { font-family: var(--mono); color: var(--text-2); text-align: right; }
  .spark { display: flex; align-items: flex-end; gap: 2px; height: 16px; }
  .spark i { width: 6px; background: var(--line-strong); border-radius: 1px; }
  .spark i.hot { background: var(--red); }

  .tool-head { display: flex; align-items: baseline; gap: 12px; }
  .tool-head .say { flex: 1; }
  .trow { display: grid; grid-template-columns: minmax(20ch, 30ch) 7ch 7ch 6ch 110px minmax(0, 1fr); align-items: center; gap: 12px; padding: 8px 8px; border-bottom: 1px solid var(--line); }
  .trow .count.warn { color: var(--amber); }
  .trow .count.bad { color: var(--red); }
  .trow .spark i.hot { background: var(--amber); }
  .tops { display: flex; flex-direction: column; gap: 2px; min-width: 0; }
  .top { text-align: left; color: var(--text-3); font-size: 12px; white-space: nowrap; overflow: hidden; text-overflow: ellipsis; padding: 1px 4px; border-radius: var(--r-sm); }
  .top:hover { background: var(--surface-hover); color: var(--text); }
  .top.muted { color: var(--muted); }
  .more { color: var(--muted); font-size: 12px; padding: 0 4px; }
  span.top:hover { background: none; color: var(--text-3); }
  .top { font-family: var(--mono); font-size: 12px; }
  .top .n { font-family: var(--mono); color: var(--muted); }
  .mono { font-family: var(--mono); }
  .review { background: var(--surface); border-radius: var(--r-md); padding: 8px 12px; margin-bottom: 12px; }
  .review .rhead { display: flex; align-items: center; gap: 6px; color: var(--muted); font-size: 12px; }
  .shadow .rhead { justify-content: space-between; }
  .review p { margin: 4px 0 0; color: var(--text-2); white-space: pre-wrap; max-width: var(--prose); }
  .finding { color: var(--text-3); font-size: 12px; white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
  /* With a problem open beside it the list is narrower: the 7 days and the ask's sentence give
     way (both are in the panel), never the title. */
  @container (max-width: 1150px) {
    .row { grid-template-columns: 30px 10px minmax(0, 1fr) minmax(12ch, 26ch) auto 5ch 8ch; }
    .row .spark { display: none; }
    .row .ask { display: none; }
  }
  @container (max-width: 820px) {
    .row { grid-template-columns: 30px 10px minmax(0, 1fr) 5ch 8ch; }
    .row .says, .row .says-h, .row .act { display: none; }
  }
</style>
