<!-- Settings > Updates: everything that can be updated, on every machine. On the left one row per program, Hub itself and each agent CLI (Claude Code, Codex, Hermes…), with how it stands on the machines. On the right the one picked: a row per machine (from → to, building, ready, waiting, failed, with Install), then what is new. Hub's news comes from its patch notes (notes.rs), each item with Show me (the page it is on) and How to test (the steps, and whether it worked); a CLI's from its makers' release notes (`GET /api/updates/news`).
     Used by Settings.svelte. -->
<script lang="ts">
  import { quiet } from "../quiet";
  import Quiet from "./Quiet.svelte";
  // Settings > Updates: everything that can be updated, on every machine. On the left one row per
  // program, Hub itself and each agent CLI (Claude Code, Codex, Hermes…), with how it stands on
  // the machines. On the right the one picked: a row per machine (from → to, building, ready,
  // waiting, failed, with Install), then what is new. Hub's news comes from its patch notes
  // (notes.rs), each item with Show me (the page it is on) and How to test (the steps, and
  // whether it worked); a CLI's from its makers' release notes (`GET /api/updates/news`).
  import Working from "./Working.svelte";
  import { onDestroy, onMount, type Snippet } from "svelte";
  import { api } from "../api";
  import type { CliNews } from "../bindings/CliNews";
  import type { Fleet } from "../bindings/Fleet";
  import type { MachineUpdates } from "../bindings/MachineUpdates";
  import type { Tool } from "../bindings/Tool";
  import { ago, clock, shortVersion } from "../format";
  import { openUrl } from "../native";
  import { noteKey, parseNotes, type NoteItem } from "../patchNotes";
  import { prefs, setPref } from "../prefs.svelte";
  import { hub } from "../state.svelte";
  import { trial, wentBack } from "../rollback";
  import Icon from "./Icon.svelte";
  import Markdown from "./Markdown.svelte";
  import Switch from "./Switch.svelte";

  /** The settings of how Hub updates, folded away under the page. */
  let { settings }: { settings?: Snippet } = $props();

  let fleet = $state<Fleet | null>(null);
  let failed = $state("");
  /** The went-back notice is closed; the core forgets it a moment later. */
  let closed = $state(false);
  let reading = $state(false);
  let starting = $state<Record<string, boolean>>({});
  let picked = $state("hubd");
  // Open when Settings' search pointed at one of the settings folded in here.
  let howOpen = $state(!!hub.settingsSpot && hub.settingsSpot !== "Install everywhere");
  let testing = $state<string | null>(null);
  let timer: ReturnType<typeof setTimeout> | undefined;
  let loading = false;
  let disposed = false;

  /** Version numbers by their numeric parts, as `updates.rs` compares them. */
  function compare(a: string, b: string): number {
    const parts = (v: string) => v.split("-")[0].split(".").map((n) => Number(n) || 0);
    const [x, y] = [parts(a), parts(b)];
    for (let i = 0; i < Math.max(x.length, y.length); i++) {
      const d = (x[i] ?? 0) - (y[i] ?? 0);
      if (d) return d;
    }
    return (a.includes("-") ? -1 : 0) - (b.includes("-") ? -1 : 0);
  }
  const when = (ms: number) => (ago(ms) === "now" ? "just now" : `${ago(ms)} ago`);
  const firstLine = (s: string | null | undefined) => (s ?? "").split("\n").find((l) => l.trim()) ?? "";

  type State = "ready" | "building" | "waiting" | "updating" | "behind" | "failed" | "current" | "offline";
  /** `dots`: how far a version sent to another machine has got (0 to 5) and `bad` when it stopped there; `tip` says when each step was. */
  type Cell = { machine: string; local: boolean; from: string; to: string; state: State; label: string; note: string; act: "install" | "update" | "retry" | "build" | null; dots?: number; bad?: boolean; tip?: string };
  type Item = { id: string; label: string; version: string; cells: Cell[] };

  const pending: State[] = ["ready", "building", "waiting", "updating", "behind", "failed"];
  const busy = $derived(hub.running.length + hub.waiting.length);

  /** The steps of a version sent to another machine, in order: each is one dot (`update_track.rs`). */
  const STEPS: Record<string, { dots: number; state: State; label: string }> = {
    sending: { dots: 0, state: "waiting", label: "sending" },
    sent: { dots: 1, state: "behind", label: "sent" },
    received: { dots: 2, state: "building", label: "received" },
    building: { dots: 3, state: "building", label: "building" },
    ready: { dots: 4, state: "ready", label: "ready" },
    restarting: { dots: 5, state: "waiting", label: "restarting" },
    done: { dots: 5, state: "current", label: "updated" },
    failed: { dots: 0, state: "failed", label: "failed" },
  };

  /** A version this machine sent to another one, as the machine itself reported it. Null when none is on the way. */
  function trackCell(m: MachineUpdates, base: Omit<Cell, "state" | "label">): Cell | null {
    const t = m.hubd_track;
    if (!t || t.step === "done") return null;
    // A track of a version this machine has moved past says nothing about the one it brings now.
    if (fleet?.hubd && shortVersion(t.version, "") !== shortVersion(fleet.hubd, "")) return null;
    const step = STEPS[t.step] ?? { dots: 0, state: "waiting" as State, label: String(t.step) };
    const failed = t.step === "failed";
    const times = [t.sent_at && `sent ${clock(t.sent_at)}`, t.received_at && `received ${clock(t.received_at)}`].filter(Boolean).join(" · ");
    return {
      ...base,
      to: shortVersion(t.version, ""),
      state: step.state,
      label: step.label,
      note: t.detail ?? "",
      act: failed ? "retry" : null,
      dots: failed ? (t.received_at ? 2 : t.sent_at ? 1 : 0) : step.dots,
      bad: failed,
      tip: [t.detail, times].filter(Boolean).join("\n"),
    };
  }

  /** Hub on one machine: this one from its own updater, the others from what they report. */
  function hubCell(m: MachineUpdates): Cell {
    const base = { machine: m.machine, local: m.local, from: shortVersion(m.hubd_state?.version ?? m.hubd, ""), to: "", note: "", act: null };
    if (m.local) {
      const u = hub.update;
      if (!u || u.version === "dev") return { ...base, from: u?.version ?? "", state: "current", label: "development build" };
      const from = shortVersion(u.version, "");
      const to = shortVersion(u.target, "");
      if (u.state === "restarting") return { ...base, from, to, state: "waiting", label: "restarting" };
      if (u.state === "ready" && hub.installAsked === u.target)
        return { ...base, from, to, state: "waiting", label: busy ? `waits for ${busy} thread${busy === 1 ? "" : "s"}` : "installing" };
      if (u.state === "ready")
        return { ...base, from, to, state: "ready", label: "ready", note: u.building ? `${shortVersion(u.building, "")} ${u.waiting_until ? `builds at ${clock(u.waiting_until)}` : "builds behind it"}` : "", act: "install" };
      // A Rust change waits for a quiet branch, so the next ones build with it; Build now skips that.
      if (u.waiting_until && (u.state === "building" || u.state === "checking"))
        return { ...base, from, to: shortVersion(u.target, ""), state: "waiting", label: `builds at ${clock(u.waiting_until)}`, note: "Rust changed: waits until the branch has been quiet, so more Rust changes build with it", act: "build" };
      if (u.state === "building" || u.state === "checking")
        return { ...base, from, to: shortVersion(u.target ?? u.building, ""), state: "building", label: u.state === "checking" ? "checking" : "building", note: u.step ?? "" };
      if (u.state === "failed") return { ...base, from, to, state: "failed", label: "failed", note: firstLine(u.message), act: "retry" };
      if (trial(u)) return { ...base, from, state: "current", label: "on trial", note: "Hub goes back to the last working version if this one breaks in the first minutes" };
      return { ...base, from, state: "current", label: "up to date" };
    }
    // A version sent here is followed by what this machine reported, step by step, and never falls back.
    const tracked = trackCell(m, base);
    if (tracked) return tracked;
    if (!m.online) return { ...base, state: "offline", label: "offline", note: m.error ?? "" };
    const st = m.hubd_state;
    if (st?.state === "building" || st?.state === "checking") return { ...base, to: shortVersion(st.target, ""), state: "building", label: "building", note: st.step ?? "" };
    if (st?.state === "failed") return { ...base, state: "failed", label: "failed", note: firstLine(st.message), act: "update" };
    if (st?.state === "ready") return { ...base, to: shortVersion(st.target, ""), state: "ready", label: "ready", act: "install" };
    if (st?.state === "restarting") return { ...base, to: shortVersion(st.target, ""), state: "waiting", label: "restarting" };
    if (fleet?.hubd && (st?.version ?? m.hubd) && (st?.version ?? m.hubd) !== fleet.hubd) return { ...base, to: shortVersion(fleet.hubd, ""), state: "behind", label: m.hubd_note?.startsWith("sent ") ? "update sent" : "different version", note: m.hubd_note ?? "", act: "update" };
    return { ...base, state: "current", label: "up to date", note: m.hubd_note ?? "" };
  }

  /** A failed update in one line: the reason the machine read from the output, or, from an older
   * hubd, the output lines that say what went wrong (npm's code, path and error), never the line
   * pointing at npm's log file. */
  function failNote(job: { message: string; lines: string[] }): string {
    if (!job.message.startsWith("The update failed:")) return job.message;
    const said = job.lines.filter((l) => !l.startsWith("$ ") && !/complete log of this run|[\\/]_logs[\\/]/.test(l));
    const telling = said.filter((l) => /\b(code|path)\s|Error:/.test(l));
    return [job.message, ...(telling.length ? telling.slice(0, 3) : said.slice(-2))].join(" · ");
  }

  function toolCell(m: MachineUpdates, t: Tool, top: string | null): Cell {
    const base = { machine: m.machine, local: m.local, from: t.version ?? "?", to: "", note: "", act: null };
    const job = t.job;
    if (!m.online) return { ...base, state: "offline", label: "offline" };
    if (job?.running) return { ...base, to: top ?? "", state: "updating", label: "updating", note: job.message };
    const old = !!t.version && !!top && compare(t.version, top) < 0;
    if (job?.ok === false) return { ...base, to: old ? top! : "", state: "failed", label: "failed", note: failNote(job), act: t.update ? "retry" : null };
    if (old) return { ...base, to: top!, state: "behind", label: "behind", act: t.update ? "update" : null, note: t.update ? "" : "Hub does not know how to update it there" };
    return { ...base, state: "current", label: "up to date", note: job?.ok ? job.message : "" };
  }

  /** The newest version of a CLI: what npm says, or the newest any machine runs. */
  function newest(id: string): string | null {
    let best: string | null = null;
    for (const m of fleet?.machines ?? []) {
      const t = m.tools.find((x) => x.id === id);
      for (const v of [t?.latest, t?.version]) if (v && (!best || compare(v, best) > 0)) best = v;
    }
    return best;
  }

  const items = $derived.by<Item[]>(() => {
    const machines = fleet?.machines ?? [];
    const u = hub.update;
    const hubItem: Item = {
      id: "hubd",
      label: "Hub",
      version: shortVersion(u?.state === "ready" ? u.target : u?.building ?? u?.target ?? u?.version ?? fleet?.hubd, ""),
      cells: machines.length ? machines.map(hubCell) : [hubCell({ machine: hub.host?.hostname ?? "This PC", local: true, online: true, hubd: u?.version ?? null, hubd_state: u, hubd_note: null, tools: [], error: null })],
    };
    const seen = new Map<string, string>();
    for (const m of machines) for (const t of m.tools) if (t.installed && !seen.has(t.id)) seen.set(t.id, t.label);
    const tools = [...seen].map(([id, label]) => {
      const top = newest(id);
      const cells = machines.flatMap((m) => {
        const t = m.tools.find((x) => x.id === id && x.installed);
        return t ? [toolCell(m, t, top)] : [];
      });
      return { id, label, version: top ?? "", cells };
    });
    return [hubItem, ...tools];
  });
  const waiting = $derived(items.filter((i) => i.cells.some((c) => pending.includes(c.state))));
  const settled = $derived(items.filter((i) => !waiting.includes(i)));
  const item = $derived(items.find((i) => i.id === picked) ?? items[0]);
  const working = $derived(Object.values(starting).some(Boolean) || items.some((i) => i.cells.some((c) => c.state === "building" || c.state === "updating")));

  /** The few words under a program's name on the left. */
  function summary(i: Item): { state: State; text: string }[] {
    const open = i.cells.filter((c) => c.state !== "current" && c.state !== "offline");
    const out = open.slice(0, 3).map((c) => ({ state: c.state, text: `${c.machine} ${c.label}` }));
    if (open.length > 3) out.push({ state: open[3].state, text: `${open.length - 3} more` });
    const ok = i.cells.filter((c) => c.state === "current").length;
    if (ok) out.push({ state: "current", text: open.length ? `${ok} up to date` : ok === i.cells.length && ok > 1 ? "all machines" : i.cells.filter((c) => c.state === "current").map((c) => c.machine).join(", ") });
    return out;
  }

  async function load(fresh = false) {
    if (loading || disposed) return;
    loading = true;
    reading = fresh;
    try {
      const next = await api.updates(fresh);
      if (disposed) return;
      fleet = next;
      failed = "";
    } catch (e) {
      failed = e instanceof Error ? e.message : String(e);
    } finally {
      reading = false;
      loading = false;
    }
    if (disposed) return;
    clearTimeout(timer);
    // Close while something runs, else now and then.
    timer = setTimeout(() => load(true), working ? 3_000 : 15_000);
  }

  async function act(i: Item, c: Cell | null) {
    const key = `${i.id}:${c?.machine ?? "*"}`;
    starting = { ...starting, [key]: true };
    try {
      if (i.id === "hubd" && (!c || c.local) && hub.update?.state === "ready") hub.installAsked = hub.update.target;
      if (i.id === "hubd" && c?.local) {
        if (c.act === "retry") await api.retryUpdate();
        else if (c.act === "build") await api.buildUpdateNow();
        else await hub.installUpdate();
      } else {
        fleet = await api.updateEverywhere(i.id, c?.machine);
      }
      clearTimeout(timer);
      timer = setTimeout(() => load(true), 3_000);
    } catch (e) {
      hub.fail(e);
    } finally {
      starting = { ...starting, [key]: false };
    }
  }

  async function follow(on: boolean) {
    try {
      fleet = await api.followUpdates(on);
    } catch (e) {
      hub.fail(e);
    }
  }

  // ---- What is new ----

  /** Hub's notes: the version on offer, else the one running. */
  const hubNotes = $derived.by(() => {
    const u = hub.update;
    const next = u?.state === "ready" ? u.target : null;
    const n = (next && hub.notes.find((x) => x.version === next)) || hub.notes.find((x) => x.version === u?.version) || null;
    return { notes: n, next: !!next && n?.version === next, waitingFor: next && n?.version !== next ? next : null };
  });
  const sections = $derived(hubNotes.notes?.text ? parseNotes(hubNotes.notes.text) : []);

  let news = $state<Record<string, CliNews | "loading">>({});
  /** A CLI's releases from the oldest any machine runs up to the newest; or, all up to date, the newest one. */
  const newsRange = $derived.by(() => {
    if (!item || item.id === "hubd") return null;
    const versions = item.cells.map((c) => c.from).filter((v) => v && v !== "?");
    const oldest = versions.sort(compare)[0] ?? null;
    const top = item.version || null;
    const behind = !!oldest && !!top && compare(oldest, top) < 0;
    return { key: `${item.id}:${behind ? oldest : ""}:${top ?? ""}`, tool: item.id, from: behind ? oldest : null, to: top, behind };
  });
  $effect(() => {
    const r = newsRange;
    if (!r || news[r.key]) return;
    news[r.key] = "loading";
    api.cliNews(r.tool, r.from, r.to).then(
      (n) => (news[r.key] = n),
      (e) => (news[r.key] = { tool: r.tool, releases: [], url: null, error: e instanceof Error ? e.message : String(e) }),
    );
  });
  const cliNews = $derived(newsRange ? news[newsRange.key] : undefined);
  let openRelease = $state<Record<string, boolean>>({});

  /** Show me: the page the change is on, with a card naming it. */
  function showMe(n: NoteItem) {
    const p = n.show;
    if (!p) return;
    if ("settings" in p) {
      hub.openSettings(p.settings);
      return;
    }
    hub.tour = { text: n.text, version: hubNotes.notes?.version ?? "" };
    hub.settingsOpen = false;
    hub.show({ view: p.view } as Parameters<typeof hub.show>[0]);
  }

  function tried(n: NoteItem, how: "ok" | "broken") {
    const version = hubNotes.notes?.version ?? "";
    setPref("tried", { ...prefs.tried, [noteKey(version, n.text)]: how });
    testing = null;
    if (how === "broken") {
      // A thread in Hub's own folder, written for you to send or change.
      const folder = hub.projects.find((p) => /[\\/]hub$/i.test(p.path))?.path ?? hub.draftFolder();
      const steps = n.test.map((s, i) => `${i + 1}. ${s}`).join("\n");
      hub.settingsOpen = false;
      hub.startDraft(folder, `After updating Hub to ${shortVersion(version)}, this does not work:\n\n${n.text}\n\nWhat I did:\n${steps}\n\nWhat happened instead: `);
    }
  }

  onMount(() => {
    load(true);
    const visible = () => { if (document.visibilityState === "visible") load(true); };
    document.addEventListener("visibilitychange", visible);
    quiet(api.notes().then((n) => (hub.notes = n)), "notes");
    return () => document.removeEventListener("visibilitychange", visible);
  });
  onDestroy(() => { disposed = true; clearTimeout(timer); });
</script>

{#snippet row(i: Item)}
  <button class="prog" class:sel={item?.id === i.id} onclick={() => (picked = i.id)}>
    <span class="top"><span class="nm">{i.label}</span><span class="mono ver" class:new={waiting.includes(i)}>{i.version}</span></span>
    {#if waiting.includes(i)}
      <span class="sum">
        {#each summary(i) as s, k (k)}<span><span class="d {s.state}"></span>{s.text}</span>{/each}
      </span>
    {/if}
  </button>
{/snippet}

{#snippet state(c: Cell)}
  <span class="pill {c.state}">{c.label}</span>
  {#if c.dots !== undefined}
    <span class="dots" class:bad={c.bad} class:ok={c.state === "current"} aria-hidden="true">{#each [1, 2, 3, 4, 5] as n (n)}<i class:on={n <= (c.dots ?? 0)}></i>{/each}</span>
    {#if c.note}<span class="step-note" class:bad={c.bad}>{c.note}</span>{/if}
  {/if}
{/snippet}

{#if wentBack(hub.update) && !closed}
  {@const back = wentBack(hub.update)!}
  <div class="back" role="status">
    <div class="what">
      <div class="name">{fleet && fleet.machines.length > 1 ? "Here: went back to" : "Went back to"} <span class="mono">{shortVersion(back.to, "")}</span></div>
      <div class="hint">{shortVersion(back.from, "")}: {back.reason}.{#if back.db_set_aside} The newest changes are kept in the backups folder as {back.db_set_aside}.{/if}</div>
    </div>
    <button class="btn small" onclick={() => { closed = true; quiet(api.dismissRolledBack(), "dismiss"); }}>Dismiss</button>
  </div>
{/if}

<div class="center">
  <div class="left">
    {#if failed}<div class="hint pad">Could not refresh; statuses may be old: {failed}</div>{/if}
    {#if waiting.length}
      <div class="group">Waiting</div>
      {#each waiting as i (i.id)}{@render row(i)}{/each}
    {/if}
    {#if settled.length}
      <div class="group">Up to date</div>
      {#each settled as i (i.id)}{@render row(i)}{/each}
    {/if}
    {#if !fleet && !failed}<Quiet kind="loading" pad />{/if}
    <div class="read">
      {#if fleet}Read {when(Number(fleet.at))}{/if}
      <button class="link" disabled={reading} onclick={() => load(true)}>{reading ? "Asking…" : "Check now"}</button>
    </div>
  </div>

  {#if item}
    {@const any = item.cells.some((c) => c.act === "install" || c.act === "update")}
    <div class="right">
      <div class="head">
        <div class="what">
          <div class="title">{item.label} {#if item.version}<span class="mono ver" class:new={waiting.includes(item)}>{item.version}</span>{/if}</div>
          <div class="hint">
            {#if item.id === "hubd"}
              {@const u = hub.update}
              You run <span class="mono">{shortVersion(u?.version, "")}</span>{#if hubNotes.notes && hubNotes.next} · {hubNotes.notes.commits.length} commits · built and tested {when(hubNotes.notes.created_at)}{/if}
            {:else}
              {item.cells.length} machine{item.cells.length === 1 ? "" : "s"} have it
            {/if}
          </div>
        </div>
        {#if any && item.cells.length > 1}
          <button class="btn primary" disabled={starting[`${item.id}:*`]} onclick={() => act(item, null)}>{starting[`${item.id}:*`] ? "Starting…" : "Install everywhere"}</button>
        {/if}
      </div>

      <div class="sub">On each machine</div>
      <div class="machines">
        {#each item.cells as c (c.machine)}
          <div class="mrow" class:off={c.state === "offline"}>
            <span class="mname">{c.machine}{#if c.local}<span class="dim">{" · here"}</span>{/if}</span>
            <span class="mono dim vers">{c.from}{#if c.to && c.to !== c.from}{" → "}<span class="to">{c.to}</span>{/if}</span>
            <span class="mstate" data-tip={c.tip || c.note || undefined}>{@render state(c)}{#if c.dots === undefined && (c.state === "building" || c.state === "updating")}<Working size={11} />{/if}</span>
            <span class="mact">
              {#if c.act}
                {@const key = `${item.id}:${c.machine}`}
                <button class="btn small" class:primary={c.act === "install"} disabled={starting[key]} onclick={() => act(item, c)}>
                  {starting[key] ? "Starting…" : c.act === "install" ? "Install" : c.act === "retry" ? "Try again" : c.act === "build" ? "Build now" : "Update"}
                </button>
              {/if}
            </span>
          </div>
        {/each}
      </div>

      {#if item.id === "hubd"}
        <div class="sub with">
          {hubNotes.next ? `What's new in ${shortVersion(hubNotes.notes?.version, "")}` : hubNotes.notes ? `In the version you run, ${shortVersion(hubNotes.notes.version, "")}` : "What's new"}
          <button class="link" onclick={() => (hub.settingsSection = "whatsnew")}>Earlier versions</button>
        </div>
        {#if hubNotes.waitingFor}<div class="hint">Writing the notes for {shortVersion(hubNotes.waitingFor, "")}.</div>{/if}
        {#if hubNotes.notes && !sections.length}
          <ul class="commits">{#each hubNotes.notes.commits.slice(0, 12) as c (c.hash)}<li>{c.subject}</li>{/each}</ul>
        {:else if hubNotes.notes}
          {#each sections as s, si (si)}
            {#if s.kind === "other" && s.title}<div class="stitle">{s.title}</div>{/if}
            {#each s.items as n, ni (ni)}
              {@const key = noteKey(hubNotes.notes.version, n.text)}
              {@const done = prefs.tried[key]}
              <div class="news">
                <span class="kind {s.kind}">{s.kind === "new" ? "new" : s.kind === "fixed" ? "fixed" : s.kind === "try" ? "try" : ""}</span>
                <span class="ntext"><Markdown text={n.text} /></span>
                <span class="nacts">
                  {#if done}<span class="done" class:bad={done === "broken"}>{done === "ok" ? "works" : "broken"}</span>{/if}
                  {#if n.show}<button class="go" onclick={() => showMe(n)}>Show me</button>{/if}
                  {#if n.test.length}<button class="go quiet" onclick={() => (testing = testing === key ? null : key)}>How to test</button>{/if}
                </span>
              </div>
              {#if testing === key}
                <div class="test">
                  <ol>{#each n.test as step, k (k)}<li><Markdown text={step} /></li>{/each}</ol>
                  <div class="tacts">
                    <button class="btn small primary" onclick={() => tried(n, "ok")}>Works</button>
                    <button class="btn small" onclick={() => tried(n, "broken")}>Doesn't work</button>
                  </div>
                </div>
              {/if}
            {/each}
          {/each}
        {/if}
      {:else}
        <div class="sub with">
          {newsRange?.behind ? `What's new since ${newsRange.from}` : `What's new in ${item.version}`}
          {#if cliNews && cliNews !== "loading" && cliNews.url}<button class="link" onclick={() => openUrl(cliNews.url!).catch((e) => hub.fail(e))}>All release notes ↗</button>{/if}
        </div>
        {#if !cliNews || cliNews === "loading"}
          <Quiet kind="loading" />
        {:else if cliNews.error}
          <div class="hint">{cliNews.error}</div>
        {:else if !cliNews.releases.length}
          <Quiet text="No notes" />
        {:else}
          {#each (newsRange?.behind ? cliNews.releases : cliNews.releases.slice(0, 1)) as r, k (r.version)}
            {@const open = openRelease[`${item.id}:${r.version}`] ?? k === 0}
            <div class="release">
              <button class="rhead" onclick={() => (openRelease[`${item.id}:${r.version}`] = !open)}>
                <span class="mono">{r.version}</span>
                <span class="chev" class:open><Icon name="chevron" size={12} /></span>
              </button>
              {#if open}<div class="rtext"><Markdown text={r.text || "_No notes._"} /></div>{/if}
            </div>
          {/each}
        {/if}
      {/if}
    </div>
  {/if}
</div>

<button class="how" onclick={() => (howOpen = !howOpen)}>
  <span>How updates work</span>
  <span class="chev" class:open={howOpen}><Icon name="chevron" size={13} /></span>
</button>
{#if howOpen}
  <div class="howbody">
    {@render settings?.()}
    <div class="line">
      <div class="what">
        <div class="name">Install when I am away</div>
        <div class="hint">A ready version installs after 3 idle minutes.</div>
      </div>
      <Switch label="Install when I am away" on={prefs.autoInstall} onchange={(on) => setPref("autoInstall", on)} />
    </div>
    {#if fleet}
      <div class="line">
        <div class="what">
          <div class="name">Keep every machine on the same versions</div>
          <div class="hint">The others follow {fleet.machines.find((m) => m.local)?.machine ?? "this machine"} within minutes.</div>
        </div>
        <Switch label="Keep every machine on the same versions" on={fleet.follow} onchange={follow} />
      </div>
    {/if}
  </div>
{/if}

<style>
  .back { display: flex; align-items: center; gap: 12px; margin-top: 18px; padding: 10px 12px; border: 1px solid var(--line); border-radius: var(--r-md); background: var(--red-bg); }
  .back .what { flex: 1; min-width: 0; }
  .back .name { color: var(--red); font-size: 14px; }
  .center { display: flex; min-height: 460px; border: 1px solid var(--line); border-radius: var(--r-md); overflow: hidden; margin-top: 18px; }
  .left { width: 260px; flex: none; border-right: 1px solid var(--line); padding: 10px 8px; display: flex; flex-direction: column; gap: 2px; background: var(--bg-sidebar); }
  .group { color: var(--muted); font-size: 12px; padding: 10px 8px 4px; }
  .prog { display: flex; flex-direction: column; gap: 4px; text-align: left; padding: 8px 8px; border-radius: var(--r-md); }
  .prog:hover { background: var(--surface); }
  .prog.sel { background: var(--surface-hover); }
  .top { display: flex; justify-content: space-between; align-items: baseline; gap: 8px; }
  .nm { color: var(--text); font-size: 14px; }
  .ver { color: var(--muted); font-size: 12px; }
  .ver.new { color: var(--accent); }
  .sum { display: flex; flex-wrap: wrap; gap: 3px 10px; color: var(--text-3); font-size: 12px; }
  .d { display: inline-block; width: 6px; height: 6px; border-radius: 50%; margin-right: 4px; vertical-align: 1px; background: var(--line-strong); }
  .d.ready, .d.behind { background: var(--accent); }
  .d.building, .d.waiting, .d.updating { background: var(--amber); }
  .d.failed { background: var(--red); }
  .d.current { background: var(--green); }
  .read { margin-top: auto; padding: 12px 8px 2px; color: var(--muted); font-size: 12px; display: flex; gap: 10px; }
  .pad { padding: 8px; }

  .right { flex: 1; min-width: 0; padding: 18px 24px 22px; }
  .head { display: flex; align-items: center; gap: 16px; }
  .head .what { flex: 1; min-width: 0; }
  .title { color: var(--text); font-size: 16px; font-weight: 600; }
  .title .ver { font-size: 14px; font-weight: 400; margin-left: 4px; }
  .hint { color: var(--muted); font-size: 13px; margin-top: 3px; }
  .dim { color: var(--muted); }
  .sub { color: var(--muted); font-size: 12px; margin: 22px 0 6px; }
  .sub.with { display: flex; justify-content: space-between; align-items: baseline; }
  .sub .link { text-transform: none; letter-spacing: 0; font-size: 12px; }
  .machines { display: flex; flex-direction: column; }
  .mrow { display: grid; grid-template-columns: minmax(0, 1fr) 150px 120px 84px; align-items: center; gap: 12px; padding: 8px 0; border-top: 1px solid var(--line); font-size: 13px; }
  .mrow:first-child { border-top: none; }
  .mrow.off { opacity: 0.55; }
  .mname { color: var(--text-2); white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
  .vers { font-size: 12px; white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
  .vers .to { color: var(--accent); }
  .mstate { display: flex; align-items: center; gap: 8px; min-width: 0; }
  .mact { text-align: right; }
  .pill { flex: none; font-size: 12px; border-radius: var(--r-lg); padding: 1px 8px; white-space: nowrap; color: var(--text-3); background: var(--surface-hover); }
  .pill.ready, .pill.behind { color: var(--accent); background: var(--accent-bg); }
  .pill.building, .pill.waiting, .pill.updating { color: var(--amber); background: var(--amber-bg); }
  .pill.failed { color: var(--red); background: var(--red-bg); }
  .pill.current { color: var(--green); background: var(--green-bg); }
  .pill.offline { color: var(--muted); }
  .dots { display: inline-flex; gap: 3px; flex: none; margin-left: 2px; }
  .dots i { width: 5px; height: 5px; border-radius: 50%; background: var(--line-strong); }
  .dots i.on { background: var(--amber); }
  .dots.ok i.on { background: var(--green); }
  .dots.bad i.on { background: var(--red); }
  .step-note { color: var(--muted); font-size: 12px; min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .step-note.bad { color: var(--red); }

  .news { display: flex; align-items: baseline; gap: 12px; padding: 8px 0; border-top: 1px solid var(--line); }
  .kind { flex: none; width: 40px; font-size: 11px; color: var(--muted); }
  .kind.new { color: var(--accent); }
  .ntext { flex: 1; min-width: 0; color: var(--text-2); font-size: 13px; line-height: 1.45; }
  .ntext :global(p) { margin: 0; }
  .nacts { flex: none; display: flex; gap: 12px; align-items: baseline; }
  .go { color: var(--accent); font-size: 12px; padding: 0; white-space: nowrap; }
  .go.quiet { color: var(--muted); }
  .go:hover { text-decoration: underline; }
  .done { font-size: 12px; color: var(--green); }
  .done.bad { color: var(--red); }
  .test { margin: 0 0 8px 52px; padding: 10px 14px; border: 1px solid var(--line); border-radius: var(--r-md); background: var(--surface); }
  .test ol { margin: 0; padding-left: 18px; color: var(--text-2); font-size: 13px; line-height: 1.6; }
  .test ol :global(p) { margin: 0; }
  .tacts { display: flex; gap: 8px; margin-top: 10px; }
  .stitle { color: var(--text); font-size: 13px; font-weight: 600; margin: 12px 0 4px; }
  .commits { margin: 0; padding-left: 18px; color: var(--text-2); font-size: 13px; line-height: 1.6; }

  .release { border-top: 1px solid var(--line); }
  .rhead { display: flex; align-items: center; gap: 8px; width: 100%; padding: 8px 0; color: var(--text-2); text-align: left; }
  .rtext { color: var(--text-2); font-size: 13px; padding: 0 0 10px; max-height: 360px; overflow: auto; }
  .rtext :global(ul) { margin: 0; padding-left: 18px; }
  .chev { display: inline-grid; color: var(--muted); transition: transform var(--dur-fast) var(--ease); }
  .chev.open { transform: rotate(90deg); }

  .how { display: flex; align-items: center; justify-content: space-between; width: 100%; padding: 14px 2px; margin-top: 18px; border-top: 1px solid var(--line); color: var(--text-2); font-size: 14px; text-align: left; }
  .how:hover { color: var(--text); }
  .howbody .line { display: flex; align-items: center; gap: 32px; padding: 14px 0; border-top: 1px solid var(--line); }
  .howbody .what { flex: 1; min-width: 0; }
  .name { color: var(--text); font-size: 14px; }
  .btn { height: 30px; padding: 0 12px; border-radius: var(--r-md); border: 1px solid var(--line-strong); color: var(--text-2); font-size: 13px; white-space: nowrap; }
  .btn:hover { background: var(--surface-hover); }
  .btn.small { height: 26px; padding: 0 10px; font-size: 13px; }
  .btn.primary { background: var(--accent-bg); border-color: transparent; color: var(--accent); }
  .btn:disabled { opacity: 0.6; }
  .link { color: var(--accent); font-size: inherit; padding: 0; }
</style>
