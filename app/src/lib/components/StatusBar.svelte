<!-- The status bar at the foot of the main window (see `statusbar.svelte.ts`). Left: where you are (machine, branch, changed files), what needs you (Guard's problems), what runs (terminals, ports) and Hub's own development (the update, the merge queue). Right: live mode. Short text, one icon at most, colour only for state; a click opens a small card or the panel above.
     Used by App.svelte. -->
<script lang="ts">
  import { quiet } from "../quiet";
  import { poll } from "../poll";
  import { pop } from "../popover";
  import Quiet from "./Quiet.svelte";
  // The status bar at the foot of the main window (see `statusbar.svelte.ts`). Left: where you are
  // (machine, branch, changed files), what needs you (Guard's problems), what runs (terminals,
  // ports) and Hub's own development (the update, the merge queue). Right: live mode. Short text,
  // one icon at most, colour only for state; a click opens a small card or the panel above.
  import Working from "./Working.svelte";
  import { api } from "../api";
  import type { ReleaseNotes } from "../bindings/ReleaseNotes";
  import type { VaktenStatus } from "../bindings/VaktenStatus";
  import { threadSays } from "../guard";
  import { ago, bytes, clock, shortVersion } from "../format";
  import { live, showingLive } from "../live.svelte";
  import { hub } from "../state.svelte";
  import { bar, onScreen } from "../statusbar.svelte";
  import Icon from "./Icon.svelte";
  import JevBar from "./JevBar.svelte";
  import PortList from "./PortList.svelte";
  import StateSign from "./StateSign.svelte";
  import { sessions, timeOf, type SessionState } from "../sessions.svelte";
  import { diskLevel, memLevel } from "../machineHealth";
  import { machineStats } from "../machineStats.svelte";
  import { GLYPHS, machineGlyph } from "../refs";

  // The Chat world has nothing of the code side in its bar: no branch, terminal, ports, queue or live.
  const chat = $derived(hub.world === "chat");
  const place = $derived(bar.place);
  const u = $derived(hub.update);
  const problems = $derived(hub.problemsNeedYou.length);
  // Guard: how many fix threads are its, and whether one waits on you.
  let guard = $state<VaktenStatus | null>(null);
  $effect(() => {
    void hub.problemsTick;
    const t = setTimeout(() => quiet(api.vakten().then((v) => (guard = v)), "vakten"), 600);
    return () => clearTimeout(t);
  });
  const guardThreads = $derived(hub.guardThreads);
  const guardWorking = $derived(guardThreads.some((t) => t.status === "running") || guard?.state === "working");
  const guardAsks = $derived(hub.guardAsks.length);
  const guardShown = $derived(!!guard?.prefs.on || guardThreads.length > 0 || guardAsks > 0);
  const ports = $derived(bar.openPorts);

  // What the page shows changes what the bar asks for.
  $effect(() => {
    void place.thread;
    void place.cwd;
    void bar.loadGit();
    const t = poll(() => bar.loadGit(), 20_000);
    return () => t();
  });
  $effect(() => {
    void place.host;
    bar.ports = null;
    void bar.loadPorts();
    const t = poll(() => bar.loadPorts(), 30_000);
    return () => t();
  });
  $effect(() => {
    void bar.loadTerminals();
    const t = poll(() => {
      void bar.loadTerminals();
    }, 20_000);
    return () => t();
  });

  // Live agent sessions: icon, colour and count in the bar, the same rows in the card.
  $effect(() => (chat ? undefined : sessions.watch(15_000)));
  // A machine that is about to run out of memory or disk shows as a red chip, from the same
  // readings the Machines page draws (machineStats.svelte.ts).
  $effect(() => (chat ? undefined : machineStats.watch(15_000)));
  const tight = $derived(
    Object.entries(machineStats.snaps).flatMap(([name, snap]) => {
      const disk = machineStats.abouts[name]?.disk;
      const out: { name: string; text: string }[] = [];
      if (snap && memLevel(snap.mem_used, snap.mem_total) === "bad") out.push({ name, text: `${name} memory ${Math.round((snap.mem_used / snap.mem_total) * 100)}%` });
      if (disk && diskLevel(disk.free, disk.total) === "bad") out.push({ name, text: `${name} disk ${bytes(disk.free)} free` });
      return out;
    }),
  );
  const SESSION_SIGNS: { k: SessionState; sign: "working" | "asks" | "idle" | "failed" | "asleep"; says: string; tone: string }[] = [
    { k: "working", sign: "working", says: "working", tone: "green" },
    { k: "asks", sign: "asks", says: "waiting on you", tone: "amber" },
    { k: "idle", sign: "idle", says: "idle, its engine stops soon", tone: "" },
    { k: "lingering", sign: "failed", says: "should be gone: settled or archived and still running", tone: "red" },
    { k: "asleep", sign: "asleep", says: "asleep, waiting on you", tone: "accent" },
  ];
  const sessionGlyph = (name: string, host: string | null) => GLYPHS[machineGlyph(name, host === null)];

  /** The update as one short phrase, and whether Hub is busy with it. */
  const update = $derived.by(() => {
    if (!u || u.version === "dev") return { text: "Development build", busy: false, tone: "" };
    // Rust held back by the build mode: how many wait of how many it takes, or that only a click builds it.
    if (u.waiting && u.waiting.mode !== "right_away" && (u.state === "building" || u.state === "checking")) {
      const { rust, goal } = u.waiting;
      return { text: goal ? `${rust} of ${goal} waiting` : u.waiting.mode === "manual" || !u.waiting_until ? `${rust} waiting` : `Builds at ${clock(u.waiting_until)}`, busy: false, tone: "amber" };
    }
    if (u.waiting_until && (u.state === "building" || u.state === "checking")) return { text: `Builds at ${clock(u.waiting_until)}`, busy: false, tone: "amber" };
    if (u.state === "building") return { text: stepShort(u.step) ?? "Building", busy: true, tone: "" };
    if (u.state === "ready") return { text: `Ready · ${shortVersion(u.target, "")}`, busy: false, tone: "accent" };
    if (u.state === "restarting") return { text: "Installing", busy: true, tone: "" };
    if (u.state === "failed") return { text: "Update failed", busy: false, tone: "red" };
    if (u.state === "checking") return { text: shortVersion(u.version, ""), busy: false, tone: "" };
    return { text: shortVersion(u.version, ""), busy: false, tone: "" };
  });

  /** "Step 3 of 5: cargo test …" → "step 3 of 5". */
  function stepShort(step: string | null | undefined): string | null {
    const m = step?.match(/step (\d+) of (\d+)/i);
    return m ? `step ${m[1]} of ${m[2]}` : null;
  }

  /** Where the version on its way is: merged, waiting for quiet, tested, built, ready. */
  const chain = $derived.by(() => {
    const steps = ["Merged", "Quiet", "Test", "Build", "Ready"];
    if (!u || !["building", "checking", "ready", "restarting"].includes(u.state) || (u.state === "checking" && !u.waiting_until)) return null;
    let at = 2;
    if (u.waiting_until) at = 1;
    else if (u.state === "ready" || u.state === "restarting") at = 5;
    else if (/build|--release/i.test(u.step ?? "") && !/test|check/i.test(u.step ?? "")) at = 3;
    return steps.map((name, i) => ({ name, state: i < at ? "done" : i === at ? "now" : "" }));
  });

  let notes = $state<ReleaseNotes[]>([]);
  $effect(() => {
    if (bar.card !== "update") return;
    quiet(api.notes().then((n) => (notes = n)), "notes");
  });
  const prs = (n: ReleaseNotes) => [...new Set(n.commits.flatMap((c) => c.subject.match(/#\d+/g) ?? []))].slice(0, 4).join(" · ");
  const earlier = $derived(notes.filter((n) => n.version !== u?.target).sort((a, b) => b.created_at - a.created_at).slice(0, 3));

  // Said once each time the count of waiting Rust changes reaches one of the notice counts (3 and 5).
  let noticed = 0;
  $effect(() => {
    const w = u?.waiting;
    if (!w || w.mode !== "merges") {
      noticed = 0;
      return;
    }
    if (w.rust > noticed && w.notice_at.includes(w.rust)) hub.tell(`${w.rust} Rust changes wait${w.goal ? `; Hub builds at ${w.goal}` : ""}`);
    noticed = w.rust;
  });

  async function buildNow() {
    try {
      await api.buildUpdateNow();
      hub.tell("Building it now");
    } catch (e) {
      hub.fail(e);
    }
  }


  function openProblems() {
    bar.card = null;
    hub.view = "problems";
    hub.selected = null;
  }

  /** The card's left edge: above its item, kept inside the window. */
  /** Where a card's left edge goes, in the page's own (zoomed) pixels, so it stays on screen. */
  const at = (width: number) => {
    const z = Number(document.documentElement.style.zoom) || 1;
    return Math.max(6, Math.min(bar.cardX / z, innerWidth / z - width - 8));
  };

  function closeOnOutside(e: MouseEvent) {
    if (!bar.card) return;
    const el = e.target as HTMLElement;
    if (!el.closest(".bar-card") && !el.closest(".statusbar")) bar.card = null;
  }
</script>

<svelte:window onmousedown={closeOnOutside} onkeydown={(e) => e.key === "Escape" && bar.card && (bar.card = null)} />

<footer class="statusbar" aria-label="Status bar">
  <button class="it machine" data-tip="Open {place.machine}'s page" onclick={() => hub.openMachine(place.machine)}>
    <Icon name="monitor" size={12} />{place.machine}
  </button>
  {#if place.cwd && bar.git?.branch && !chat}
    <span class="it static" data-tip="{place.label}: branch {bar.git.branch}{bar.git.changed ? `, ${bar.git.changed} changed files` : ''}">
      <Icon name="branch" size={12} /><span class="mono branch">{bar.git.branch}</span>
      {#if bar.git.changed}<span class="dim">{bar.git.changed} changed</span>{/if}
    </span>
  {/if}
  {#if problems && !chat}
    <button class="it" data-tip="{problems} problem{problems === 1 ? '' : 's'} need you (Guard)" onclick={openProblems}>
      <Icon name="alert" size={12} /><span class="amber">{problems}</span>
    </button>
  {/if}
  {#if guardShown && !chat}
    <button class="it" class:on={bar.card === "guard"} data-tip="Guard: its fix threads and what waits on you" onclick={(e) => bar.toggleCard("guard", e.currentTarget.getBoundingClientRect().left)}>
      {#if guardWorking}<Working size={11} />{:else}<Icon name="shield" size={12} />{/if}
      <span class:green={guardWorking}>{guardThreads.length ? `${guardThreads.length} ${guardThreads.length === 1 ? "thread" : "threads"}` : "Guard"}</span>
      {#if guardAsks}<span class="amber">· {guardAsks} {guardAsks === 1 ? "needs" : "need"} you</span>{/if}
    </button>
  {/if}
  {#if !chat}
  <span class="sep"></span>
  <button class="it" class:on={bar.panel === "terminal"} data-tip="Terminal on {place.machine}{place.cwd ? `, in ${place.label}` : ''} (Ctrl Shift §)" onclick={() => bar.togglePanel("terminal")}>
    <Icon name="terminal" size={12} />Terminal{#if bar.shells.length}<span class="dim">{bar.shells.length}</span>{/if}
  </button>
  <button class="it" class:on={bar.card === "ports"} data-tip="What listens on {place.machine}" onclick={(e) => bar.toggleCard("ports", e.currentTarget.getBoundingClientRect().left)}>
    <Icon name="globe" size={12} />Ports{#if bar.ports}<span class="dim">{ports.filter((p) => !p.is_hub).length}</span>{/if}
  </button>
  {/if}
  <span class="sep"></span>
  <button class="it" class:on={bar.card === "update"} data-tip={u?.step ?? "Hub's update"} onclick={(e) => bar.toggleCard("update", e.currentTarget.getBoundingClientRect().left)}>
    {#if update.busy}<Working size={11} />{:else}<Icon name="download" size={12} />{/if}
    <span class:amber={update.tone === "amber"} class:accent={update.tone === "accent"} class:red={update.tone === "red"}>{update.text}</span>
  </button>
  <span class="gap"></span>
  <JevBar />
  {#if !chat && sessions.loaded}
    <button class="it sessions" class:on={bar.card === "sessions"} data-tip="Live agent sessions on every machine" onclick={(e) => bar.toggleCard("sessions", e.currentTarget.getBoundingClientRect().left)}>
      {#each SESSION_SIGNS as g (g.k)}
        {@const n = sessions.count(g.k)}
        {#if n}<span class="count {g.tone}" data-tip="{n} {g.says}"><StateSign kind={g.sign} size={13} />{n}</span>{/if}
      {:else}
        <span class="count" data-tip="No agent session is running"><StateSign kind="idle" size={13} />0</span>
      {/each}
    </button>
  {/if}
  {#if !chat}
    {#each tight as c (c.text)}
      <button class="it tight" data-tip="Open {c.name}" onclick={() => hub.openMachine(c.name)}><Icon name="alert" size={12} />{c.text}</button>
    {/each}
  {/if}
  {#if live.offered && !chat}
    <button class="it" class:accent={showingLive} data-tip={showingLive ? "Live: the open pull requests. Click for the installed pages" : "Show the open pull requests live (Ctrl Shift L)"} onclick={() => live.toggle()}>
      <Icon name="live" size={12} />{showingLive ? `Live · ${live.included.length}` : "Live"}
    </button>
  {/if}
</footer>

{#if bar.card === "ports"}
  <div transition:pop|global class="bar-card" style:left="{at(340)}px" use:onScreen={bar.cardX}>
    <div class="head"><b>Ports on {place.machine}</b><button class="link" onclick={() => { bar.card = null; hub.view = "services"; }}>Services</button></div>
    {#if bar.portsFailed && !bar.ports}<p class="quiet">Could not read {place.machine}'s ports.</p>
    {:else if !bar.ports}<Quiet kind="loading" />
    {:else}<PortList ports={ports} host={place.host} />{/if}
  </div>
{:else if bar.card === "sessions"}
  <div class="bar-card wide" style:left="{at(380)}px" use:onScreen={bar.cardX}>
    {#each sessions.sessions as s (`${s.host ?? ""}|${s.id}`)}
      {@const sign = SESSION_SIGNS.find((g) => g.k === s.state)!}
      {@const t = timeOf(s, hub.now)}
      <button class="qrow" data-tip={t.says} onclick={() => { bar.card = null; if (s.thread) hub.open(s.id); else hub.openMachine(s.machine); }}>
        <StateSign kind={sign.sign} size={14} /><span class="grow t" class:dim={!s.thread}>{s.thread?.title ?? "A removed thread"}</span>
        <svg class="mglyph" viewBox="0 0 24 24" data-tip={s.machine}><path d={sessionGlyph(s.machine, s.host)} /></svg>
        <span class="mono when {sign.tone}">{t.text}</span>
      </button>
    {:else}
      <Quiet text="Nothing is running" />
    {/each}
    <div class="foot"><span class="dim mono grow">{bytes(sessions.memory)}</span><button class="link" onclick={() => { bar.card = null; hub.view = "machines"; hub.selected = null; }}>Machines</button></div>
  </div>
{:else if bar.card === "guard"}
  <div class="bar-card wide" style:left="{at(380)}px" use:onScreen={bar.cardX}>
    <div class="head"><b>Guard</b><span class="dim">{guardWorking ? "working" : guard?.state === "waiting" ? "waits for you" : guard?.prefs.on ? "idle" : "off"}</span></div>
    {#each guardThreads.slice(0, 8) as t (t.id)}
      {@const [tone, says] = threadSays(t)}
      <button class="qrow" data-tip="Open the thread" onclick={() => { bar.card = null; hub.open(t.id); }}>
        <span class="dot {t.status}"></span><span class="grow t">{t.title}</span><span class="pill {tone === 'work' ? 'green' : tone === 'ask' ? 'amber' : tone === 'bad' ? 'red' : ''}">{says}</span>
      </button>
    {:else}
      <Quiet text="No fix threads" />
    {/each}
    {#if guardThreads.length > 8}<p class="quiet">and {guardThreads.length - 8} more in Problems</p>{/if}
    <div class="foot">
      {#if hub.guardChat}<button class="link" onclick={() => { bar.card = null; hub.open(hub.guardChat!.id); }}>Open Guard's chat</button>{/if}
      <button class="link" onclick={openProblems}>Problems ›</button>
    </div>
  </div>
{:else if bar.card === "update"}
  <div class="bar-card wide" style:left="{at(380)}px" use:onScreen={bar.cardX}>
    <div class="head">
      <b>{!u || u.version === "dev" ? "Development build" : u.state === "ready" ? `Ready: ${shortVersion(u.target, "")}` : u?.state === "failed" ? "The update failed" : chain ? `Update to ${shortVersion(u?.target ?? u?.building, "")}` : `Hub ${shortVersion(u?.version, "")}`}</b>
      <span class="dim">{u?.checked_at ? `looked ${ago(u.checked_at)}` : ""}</span>
    </div>
    {#if chain}
      <div class="chain">{#each chain as s, i (s.name)}{#if i}<span class="arrow">›</span>{/if}<span class="step {s.state}">{s.name}</span>{/each}</div>
    {/if}
    {#if u?.waiting && u.waiting.mode !== "right_away"}
      <div class="line"><span class="grow">{u.step}</span><button class="small" onclick={buildNow}>Build now</button></div>
    {:else if u?.waiting_until}
      <div class="line"><span>Rust changed: builds at {clock(u.waiting_until)}, so more Rust changes join it</span><button class="small" onclick={buildNow}>Build now</button></div>
    {:else if u?.step}
      <div class="line"><Working size={11} /><span class="grow">{u.step}</span></div>
    {/if}
    {#if u?.state === "ready"}
      <div class="line"><span class="grow">{u.building ? `${shortVersion(u.building, "")} builds behind it` : "Built and tested"}</span><button class="small primary" onclick={() => hub.installUpdate()}>Install</button></div>
    {:else if u?.state === "failed"}
      <div class="line"><span class="grow red">{(u.message ?? "").split("\n")[0]}</span><button class="small" onclick={() => api.retryUpdate().catch((e) => hub.fail(e))}>Try again</button></div>
    {:else if !u || u.version === "dev"}
      <div class="line"><span class="grow dim">A development build does not update itself.</span></div>
    {:else if !chain}
      <div class="line"><span class="grow dim">Up to date with main.</span></div>
    {/if}
    {#if earlier.length}
      <div class="sub">Before this</div>
      {#each earlier as n (n.version)}
        <div class="hist"><span class="mono">{shortVersion(n.version, "")}</span><span class="dim">{clock(n.created_at)}</span><span class="grow">{prs(n) || `${n.commits.length} commits`}</span></div>
      {/each}
    {/if}
    <div class="foot"><button class="link" onclick={() => { bar.card = null; hub.openSettings("updates"); }}>Open Updates</button></div>
  </div>
{/if}

<style>
  .statusbar {
    position: fixed; left: 0; right: 0; bottom: 0; height: var(--bar-h); z-index: 50;
    display: flex; align-items: center; gap: 1px; padding: 0 6px 0 0;
    background: var(--bg-sidebar); border-top: 1px solid var(--line); font-size: 12px; color: var(--muted);
  }
  .it { display: inline-flex; align-items: center; gap: 4px; height: calc(var(--bar-h) - 4px); padding: 0 8px; border-radius: var(--r-xs); white-space: nowrap; color: var(--muted); font-size: 12px; }
  button.it:hover, .it.on { background: var(--surface-hover); color: var(--text-2); }
  .it.static { cursor: default; }
  .machine { height: var(--bar-h); border-radius: 0; padding: 0 10px; background: var(--accent-bg); color: var(--accent); }
  button.machine:hover { color: var(--accent); filter: brightness(1.15); }
  .branch { max-width: 240px; overflow: hidden; text-overflow: ellipsis; color: var(--text-3); }
  .mono { font-family: var(--mono); font-size: 12px; }
  .dim { color: var(--muted); }
  .amber { color: var(--amber); }
  .accent { color: var(--accent); }
  .green { color: var(--green); }
  .red { color: var(--red); }
  .sep { width: 1px; height: 12px; background: var(--line-strong); margin: 0 4px; }
  .gap { flex: 1; }

  .bar-card {
    position: fixed; bottom: calc(var(--bar-h) + 6px); z-index: 60; width: 340px; max-height: 60vh; overflow: auto;
    background: var(--surface); border: 1px solid var(--line-strong); border-radius: var(--r-md); padding: 8px;
    box-shadow: 0 10px 30px rgba(0, 0, 0, 0.45); font-size: 13px; color: var(--text-2);
  }
  .bar-card.wide { width: 380px; }
  .head { display: flex; align-items: baseline; justify-content: space-between; gap: 8px; padding: 2px 6px 6px; }
  .head b { font-weight: 600; color: var(--text); font-size: 13px; }
  .chain { display: flex; align-items: center; gap: 3px; padding: 2px 6px 6px; }
  .step { font-size: 12px; padding: 1px 6px; border-radius: var(--r-xs); color: var(--muted); }
  .step.done { color: var(--green); }
  .step.now { color: var(--amber); background: var(--amber-bg); }
  .arrow { color: var(--line-strong); font-size: 11px; }
  .line { display: flex; align-items: center; gap: 8px; padding: 4px 6px; min-height: 26px; }
  .grow { flex: 1; min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .sub { padding: 8px 6px 2px; font-size: 11px; color: var(--muted); border-top: 1px solid var(--line); margin-top: 4px; }
  .hist { display: flex; gap: 8px; align-items: baseline; padding: 3px 6px; color: var(--text-3); }
  .qrow { display: flex; align-items: center; gap: 8px; width: 100%; height: 26px; padding: 0 6px; border-radius: var(--r-sm); text-align: left; color: var(--text-2); font-size: 13px; }
  .qrow:hover { background: var(--surface-hover); }
  .qrow .n { color: var(--muted); flex: none; }
  .pill { flex: none; font-size: 11px; padding: 0 6px; border-radius: var(--r-md); color: var(--muted); background: var(--surface-hover); }
  .pill.green { color: var(--green); background: var(--green-bg); }
  .pill.amber { color: var(--amber); background: var(--amber-bg); }
  .pill.red { color: var(--red); background: var(--red-bg); }
  .foot { display: flex; justify-content: flex-end; gap: 14px; padding: 6px 6px 0; border-top: 1px solid var(--line); margin-top: 4px; }
  .link { font-size: 12px; color: var(--muted); padding: 0; }
  .link:hover { color: var(--text-2); }
  .small { flex: none; height: 22px; padding: 0 8px; border-radius: var(--r-sm); border: 1px solid var(--line-strong); font-size: 12px; color: var(--text-2); }
  .small:hover { background: var(--surface-hover); }
  .small.primary { border-color: transparent; background: var(--accent-bg); color: var(--accent); }
  .quiet { margin: 4px 6px; color: var(--muted); }
  .count { display: inline-flex; align-items: center; gap: 4px; font-family: var(--mono); font-size: 12px; color: var(--muted); }
  .count.green { color: var(--green); }
  .count.amber { color: var(--amber); }
  .count.red { color: var(--red); }
  .count.accent { color: var(--accent); }
  .sessions { gap: 8px; }
  .tight { color: var(--red); }
  .mglyph { width: 13px; height: 13px; flex: none; fill: none; stroke: var(--muted); stroke-width: 1.75; stroke-linecap: round; stroke-linejoin: round; }
  .when { flex: none; min-width: 38px; text-align: right; color: var(--text-3); font-size: 12px; }
  .when.green { color: var(--green); }
  .when.amber { color: var(--amber); }
  .when.red { color: var(--red); }
  .when.accent { color: var(--accent); }
</style>
