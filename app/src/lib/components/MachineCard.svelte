<!-- One machine as a card: what it is, one line on how it is doing, what is free of memory, CPU and disk (small waves for the first two, machineHealth.ts for the colours), and the agent sessions that run on it. The Machines page shows one per machine; a machine's own page shows it as the top of its left column (`side`), with a row each for the free capacity.
     Used by MachineView.svelte, MachinesView.svelte. -->
<script lang="ts">
  import Quiet from "./Quiet.svelte";
  // One machine as a card: what it is, one line on how it is doing, what is free of memory, CPU
  // and disk (small waves for the first two, machineHealth.ts for the colours), and the agent
  // sessions that run on it. The Machines page shows one per machine; a machine's own page shows
  // it as the top of its left column (`side`), with a row each for the free capacity.
  import type { HostInfo } from "../bindings/HostInfo";
  import type { HostSnapshot } from "../bindings/HostSnapshot";
  import type { MachineAbout } from "../bindings/MachineAbout";
  import { bytes, shortVersion } from "../format";
  import { cpuLevel, diskLevel, memLevel, tempLevel, type Level, type Signal } from "../machineHealth";
  import { GLYPHS, machineGlyph } from "../refs";
  import type { Session } from "../sessions.svelte";
  import Icon from "./Icon.svelte";
  import SessionRow from "./SessionRow.svelte";
  import type { UpdateControl } from "../machineUpdate";
  import Wave from "./Wave.svelte";
  import type { Snippet } from "svelte";

  let {
    name,
    local,
    h,
    snap,
    about,
    say,
    behind = false,
    update = null,
    version = null,
    threads = 0,
    projects = 0,
    mem = [],
    cpu = [],
    rows = null,
    side = false,
    onopen,
    actions,
  }: {
    name: string;
    local: boolean;
    h: HostInfo | undefined;
    snap: HostSnapshot | null;
    about: MachineAbout | null;
    /** The one line: worst signal first (`verdict` in machineHealth.ts). */
    say: Signal;
    behind?: boolean;
    /** The update control at the top right (the machine's own page): a button, then the step it is at with five dots, then Try again. */
    update?: UpdateControl | null;
    version?: string | null;
    threads?: number;
    projects?: number;
    /** Memory and CPU as shares over the last hour, for the small waves. */
    mem?: (number | null)[];
    cpu?: (number | null)[];
    /** The agent sessions running there; null leaves the list out. */
    rows?: Session[] | null;
    side?: boolean;
    /** Makes the whole card open the machine's page. */
    onopen?: () => void;
    /** The buttons along the bottom. */
    actions?: Snippet;
  } = $props();

  const online = $derived(local || h?.state === "online");
  const disk = $derived(about?.disk ?? null);
  const diskUsed = $derived(disk ? ((disk.total - disk.free) / Math.max(disk.total, 1)) * 100 : 0);
  const ports = $derived(snap ? snap.listeners.filter((l) => !l.system).length : 0);
  const os = $derived(about?.os ?? snap?.os ?? (local ? "" : h?.address ?? ""));
  const GB = 2 ** 30;
  const gb = (n: number) => (n / GB).toFixed(1);
  const memL = $derived(snap ? memLevel(snap.mem_used, snap.mem_total) : "ok");
  const cpuL = $derived(snap ? cpuLevel(snap.cpu_percent) : "ok");
  const diskL = $derived(disk ? diskLevel(disk.free, disk.total) : "ok");
  const cores = $derived(about?.cores ?? null);
  /** CPU free, in cores when the machine says how many it has. */
  const cpuFree = $derived(snap ? (cores ? (Math.max(0, 100 - snap.cpu_percent) / 100) * cores : Math.max(0, 100 - snap.cpu_percent)) : 0);

  const tone = (l: Level, good?: boolean) => (l === "bad" ? "bad" : l === "warn" ? "warn" : good ? "good" : "");

  function key(e: KeyboardEvent) {
    if (onopen && (e.key === "Enter" || e.key === " ") && e.target === e.currentTarget) {
      e.preventDefault();
      onopen();
    }
  }
</script>

<section
  class="card"
  class:side
  class:open={!!onopen}
  class:off={!online}
  class:alarm={online && say.level === "bad"}
  {...onopen ? { role: "link", tabindex: 0 } : {}}
  onclick={onopen}
  onkeydown={key}
>
  <div class="head">
    <svg class="kind" viewBox="0 0 24 24" aria-hidden="true"><path d={GLYPHS[machineGlyph(name, local)]} /></svg>
    <div class="who">
      <span class="name">{name}</span>
      <span class="os">{local ? "This computer" : ""}{local && os ? " · " : ""}{os}</span>
    </div>
    {#if update}
      <span role="presentation" class="upd" onclick={(e) => e.stopPropagation()}>
        {#if update.step}
          <span class="prog" data-tip={update.tip || undefined}>
            <span class="pill {update.tone}">{update.step}</span>
            <span class="dots" class:bad={update.tone === "bad"} class:ok={update.tone === "ok"} aria-hidden="true">{#each [1, 2, 3, 4, 5] as n (n)}<i class:on={n <= update.dots}></i>{/each}</span>
          </span>
        {/if}
        {#if update.button}
          <button class="upbtn" class:bad={update.tone === "bad"} disabled={update.busy} data-tip={update.tip || undefined} onclick={update.run}>
            {#if !update.step && !update.busy}<Icon name="download" size={12} />{/if}{update.busy ? "Sending…" : update.button}
          </button>
        {/if}
      </span>
    {:else if behind}<span class="ver warn" data-tip="A newer hubd is on this computer">update</span>{:else if version && !side}<span class="ver" data-tip="hubd version">{shortVersion(version)}</span>{/if}
    {#if !side && say.level === "ok"}<span class="verdict {tone(say.level, say.good)}" data-tip={say.text}><i></i><span>{say.text}</span></span>{/if}
  </div>

  {#if side || say.level !== "ok"}
    <div class="say {tone(say.level, say.good)}"><i></i><span>{say.text}</span></div>
  {/if}

  {#if online && snap}
    <div class="trio">
      <div class="tr">
        <div class="l">Memory free</div>
        <div class="v {tone(memL)}">{gb(snap.mem_total - snap.mem_used)}<small> / {gb(snap.mem_total)} GB</small></div>
        <div class="sp"><Wave values={mem} mini /></div>
      </div>
      <div class="tr">
        <div class="l">CPU free</div>
        <div class="v {tone(cpuL)}">{cores ? cpuFree.toFixed(1) : Math.round(cpuFree)}<small>{cores ? ` / ${cores}` : " %"}</small></div>
        <div class="sp"><Wave values={cpu} mini warn={0.9} bad={2} tone="blue" /></div>
      </div>
      <div class="tr">
        <div class="l">Disk free</div>
        {#if disk}
          <div class="v {tone(diskL)}">{gb(disk.free)}<small> GB</small></div>
          <div class="sp"><span class="dbar {tone(diskL)}"><i style:width="{diskUsed}%"></i></span></div>
        {:else}
          <div class="v none"><small>Update hubd to see it</small></div>
        {/if}
      </div>
    </div>
  {:else if online}
    <Quiet kind="loading" />
  {/if}

  {#if side && online && snap}
    <div class="meta">
      <span><Icon name="threads" size={13} />{threads}</span>
      <span><Icon name="folder" size={13} />{projects}</span>
      <span><svg class="g" viewBox="0 0 24 24" aria-hidden="true"><path d={GLYPHS.port} /></svg>{ports}</span>
      {#if about?.temp_c != null}<span class={tone(tempLevel(about.temp_c, about))} data-tip="Chip temperature">{Math.round(about.temp_c)} °C</span>{/if}
    </div>
  {/if}

  {#if rows}
    <div class="ses">
      {#each rows as s (`${s.host ?? ""}|${s.id}`)}
        <SessionRow {s} />
      {:else}
        <div class="none">{online ? "No agent session running" : ""}</div>
      {/each}
    </div>
  {/if}

  {#if actions}
    <div role="presentation" class="acts" onclick={(e) => e.stopPropagation()}>{@render actions()}</div>
  {/if}
</section>

<style>
  .card { background: var(--surface); border: 1px solid var(--line); border-radius: var(--r-lg); padding: 12px 16px 12px; display: flex; flex-direction: column; gap: 10px; min-width: 0; }
  .card.open { cursor: pointer; }
  .card.open:hover { border-color: var(--line-strong); }
  .card.open:focus-visible { outline: 2px solid var(--accent); outline-offset: 2px; }
  .card.off { opacity: 0.85; }
  .card.alarm { border-color: color-mix(in srgb, var(--red) 45%, transparent); background: linear-gradient(var(--red-bg), var(--red-bg)), var(--surface); }
  .head { display: flex; gap: 10px; align-items: center; min-width: 0; }
  .kind { width: 24px; height: 24px; flex: none; fill: none; stroke: var(--text-3); stroke-width: 1.75; stroke-linecap: round; stroke-linejoin: round; }
  .off .kind { stroke: var(--muted); }
  .who { display: flex; flex-direction: column; min-width: 0; flex: 1; }
  .name { color: var(--text); font-size: 15px; font-weight: 500; white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
  .os { color: var(--muted); font-size: 12px; white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
  .ver { color: var(--muted); font: 11px var(--mono); align-self: flex-start; }
  .ver.warn { color: var(--amber); }
  .upd { display: inline-flex; align-items: center; gap: 8px; flex: none; }
  .prog { display: inline-flex; align-items: center; gap: 6px; white-space: nowrap; }
  .pill { font-size: 12px; border-radius: var(--r-lg); padding: 1px 8px; color: var(--text-3); background: var(--surface-hover); }
  .pill.go { color: var(--accent); background: var(--accent-bg); }
  .pill.work { color: var(--amber); background: var(--amber-bg); }
  .pill.ok { color: var(--green); background: var(--green-bg); }
  .pill.bad { color: var(--red); background: var(--red-bg); }
  .dots { display: inline-flex; gap: 3px; }
  .dots i { width: 5px; height: 5px; border-radius: 50%; background: var(--line-strong); }
  .dots i.on { background: var(--amber); }
  .dots.ok i.on { background: var(--green); }
  .dots.bad i.on { background: var(--red); }
  .upbtn { display: inline-flex; align-items: center; gap: 6px; font-size: 12px; color: var(--amber); background: transparent; border: 1px solid var(--amber); border-radius: var(--r-md); padding: 2px 8px; cursor: pointer; white-space: nowrap; }
  .upbtn:hover:not(:disabled) { background: var(--amber-bg); }
  .upbtn.bad { color: var(--red); border-color: var(--red); }
  .upbtn.bad:hover:not(:disabled) { background: var(--red-bg); }
  .upbtn:disabled { opacity: 0.6; cursor: default; }
  .verdict { display: inline-flex; align-items: center; gap: 6px; font-size: 12px; color: var(--text-3); min-width: 0; max-width: 45%; }
  .verdict span { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .verdict i, .say i { width: 7px; height: 7px; border-radius: 50%; flex: none; background: var(--text-3); }
  .verdict.good i, .say.good i { background: var(--green); }
  .verdict.warn { color: var(--amber); }
  .verdict.warn i, .say.warn i { background: var(--amber); }
  .verdict.bad { color: var(--red); }
  .verdict.bad i, .say.bad i { background: var(--red); }

  .say { display: flex; gap: 8px; align-items: center; font-size: 13px; color: var(--text-2); padding: 8px 10px; border-radius: var(--r-md); background: var(--bg); min-width: 0; }
  .say span { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .say.warn { background: var(--amber-bg); color: var(--amber); }
  .say.bad { background: var(--red-bg); color: var(--red); }

  .trio { display: grid; grid-template-columns: repeat(3, minmax(0, 1fr)); gap: 12px; margin: 2px 0; }
  .tr { min-width: 0; }
  .l { color: var(--muted); font-size: 11px; }
  .v { font-family: var(--mono); font-size: 16px; color: var(--text); margin: 1px 0 4px; white-space: nowrap; }
  .v small { font-size: 11px; color: var(--muted); }
  .v.warn { color: var(--amber); }
  .v.bad { color: var(--red); }
  .sp { height: 32px; display: flex; align-items: center; }
  .sp > :global(svg) { height: 100%; }
  .dbar { height: 5px; border-radius: 3px; background: var(--line); overflow: hidden; width: 100%; }
  .dbar i { display: block; height: 100%; background: var(--text-3); opacity: 0.6; }
  .dbar.warn i { background: var(--amber); opacity: 0.9; }
  .dbar.bad i { background: var(--red); opacity: 0.9; }
  .card.side .trio { grid-template-columns: 1fr; gap: 8px; }
  .card.side .tr { display: grid; grid-template-columns: 76px minmax(0, 1fr) 96px; gap: 10px; align-items: center; }
  .card.side .v { grid-column: 3; grid-row: 1; text-align: right; margin: 0; }
  .card.side .sp { grid-column: 2; grid-row: 1; height: 22px; }

  .meta { display: flex; gap: 14px; flex-wrap: wrap; color: var(--text-3); font-size: 12px; }
  .meta span { display: inline-flex; align-items: center; gap: 4px; }
  .meta .g { width: 13px; height: 13px; fill: none; stroke: currentColor; stroke-width: 1.75; stroke-linecap: round; stroke-linejoin: round; }
  .meta .warn { color: var(--amber); }
  .meta .bad { color: var(--red); }
  .wait { color: var(--muted); font-size: 13px; margin: 0; }

  .ses { border-top: 1px solid var(--line); padding-top: 6px; display: flex; flex-direction: column; }
  .none { color: var(--muted); font-size: 12px; padding: 4px 0; }

  .acts { display: flex; gap: 6px; border-top: 1px solid var(--line); padding-top: 10px; cursor: default; }
  .acts :global(.btn) { flex: 1; justify-content: center; display: inline-flex; align-items: center; gap: 6px; height: 28px; padding: 0 8px; font-size: 13px; min-width: 0; white-space: nowrap; }
  .acts :global(.btn.more) { flex: 0 0 auto; padding: 0 6px; }
</style>
