<!-- Every machine threads can run on: the whole network's memory and CPU as two waves, how many more agents it has room for, and a card per machine with what is free there, the waves of its last hour and the agent sessions running on it (machineStats.svelte.ts, sessions.svelte.ts). The waves turn amber and red as a machine fills (machineHealth.ts). The card opens the machine's own page; adding, pairing and installing stay in Settings.
     Used by App.svelte. -->
<script lang="ts">
  import Page from "./Page.svelte";
  import { recall, remember } from "../lastSeen";
  import { quiet } from "../quiet";
  import { poll } from "../poll";
  import { contextMenu, copy, menu, type MenuEntry } from "../menu.svelte";
  import { machineKindMenu } from "../menus.svelte";
  import { onMount } from "svelte";
  import { api } from "../api";
  import type { EngineCli } from "../bindings/EngineCli";
  import type { HostInfo } from "../bindings/HostInfo";
  import type { HostSnapshot } from "../bindings/HostSnapshot";
  import type { MachineAbout } from "../bindings/MachineAbout";
  import type { MachineReady } from "../bindings/MachineReady";
  import type { OpenOnMachine } from "../bindings/OpenOnMachine";
  import { bytes, threadProject } from "../format";
  import { signals, verdict } from "../machineHealth";
  import { machineStats, RANGES, type Range } from "../machineStats.svelte";
  import { sessions } from "../sessions.svelte";
  import { hub } from "../state.svelte";
  import Icon from "./Icon.svelte";
  import MachineCard from "./MachineCard.svelte";
  import Wave from "./Wave.svelte";

  // Every machine threads can run on: the whole network's memory and CPU as two waves, how many
  // more agents it has room for, and a card per machine with what is free there, the waves of its
  // last hour and the agent sessions running on it (machineStats.svelte.ts, sessions.svelte.ts).
  // The waves turn amber and red as a machine fills (machineHealth.ts). The card opens the
  // machine's own page; adding, pairing and installing stay in Settings.
  const snaps = $derived(machineStats.snaps);
  const abouts = $derived(machineStats.abouts);
  let readies = $state<Record<string, MachineReady | null>>(recall("machine.ready") ?? {});
  let versions = $state<Record<string, string | null>>(recall("machine.versions") ?? {});
  let ours = $state<string | null>(null);
  let clis = $state<EngineCli[]>([]);

  const online = () => hub.hosts.filter((h) => h.local || h.state === "online");
  const at = (h: HostInfo) => (h.local ? null : h.name);

  /** Asks every online machine; one that does not answer keeps what it said last (lastSeen.ts). */
  async function each<T>(key: string, prev: Record<string, T | null>, get: (h: HostInfo) => Promise<T>): Promise<Record<string, T | null>> {
    const on = online();
    const got = await Promise.all(on.map((h) => quiet(get(h), "get", undefined)));
    const out = Object.fromEntries(on.map((h, i) => [h.name, got[i] === undefined ? (prev[h.name] ?? null) : got[i]!]));
    remember(key, out);
    return out;
  }
  const readReady = async () => (readies = await each("machine.ready", readies, (h) => api.machineReady(at(h))));
  const readVersions = async () => (versions = await each("machine.versions", versions, async (h) => (h.local ? null : (await api.version(h.name)).version)));

  onMount(() => {
    readReady();
    readVersions();
    quiet(api.version().then((v) => (ours = v.version)), "version");
    quiet(api.engineClis().then((c) => (clis = c)), "engine clis");
    const timers = [poll(readVersions, 30_000), poll(readReady, 120_000)];
    // The waves and the sessions move every few seconds while this page is open.
    const stops = [machineStats.watch(3000), sessions.watch(3000)];
    // Back from a terminal where something was signed in: look again.
    window.addEventListener("focus", readReady);
    return () => {
      timers.forEach((stop) => stop());
      stops.forEach((stop) => stop());
      window.removeEventListener("focus", readReady);
    };
  });

  // A machine that just came online is read at once instead of on the next tick.
  $effect(() => {
    const names = online().map((h) => h.name).join("\u0000");
    void names;
    void machineStats.pull();
    readReady();
  });

  // A thread with no host runs here; until the hosts list has come, this machine has no name yet.
  const machines = $derived<HostInfo[]>(
    hub.hosts.length
      ? hub.hosts
      : [{ name: hub.host?.hostname ?? "This computer", local: true, address: "", state: "online", version: null, error: null, removable: false, setup: null, join: null, sending: null }],
  );

  let range = $state<Range>("1h");
  const names = $derived(machines.filter((h) => h.local || h.state === "online").map((h) => h.name));
  const netMem = $derived(machineStats.network(names, range, "mem"));
  const netCpu = $derived(machineStats.network(names, range, "cpu"));
  const total = $derived.by(() => {
    let mem = 0, used = 0, cores = 0, busy = 0, count = 0;
    for (const n of names) {
      const sn = snaps[n];
      if (!sn) continue;
      count++;
      mem += sn.mem_total;
      used += sn.mem_used;
      const c = abouts[n]?.cores ?? 1;
      cores += c;
      busy += (sn.cpu_percent / 100) * c;
    }
    return { mem, used, cores, busy, count };
  });
  /** How many more agents fit: what is free on each machine, less a tenth kept back, over what one takes now. */
  const room = $derived.by(() => {
    const GB = 2 ** 30;
    const taking = sessions.sessions.filter((s) => s.mem > 0);
    const each = Math.min(3 * GB, Math.max(0.6 * GB, taking.length ? taking.reduce((n, s) => n + s.mem, 0) / taking.length : 1.2 * GB));
    let n = 0;
    let best: { name: string; n: number } | null = null;
    for (const name of names) {
      const sn = snaps[name];
      if (!sn) continue;
      const fit = Math.max(0, Math.floor((sn.mem_total - sn.mem_used - sn.mem_total * 0.1) / each));
      n += fit;
      if (!best || fit > best.n) best = { name, n: fit };
    }
    return total.count ? { n, each, best: best?.name ?? null } : null;
  });

  function threadsOn(h: HostInfo) {
    const on = hub.active.filter((t) => (h.local ? !t.host || t.host === h.name : t.host === h.name));
    return {
      all: on.length,
      working: on.filter((t) => t.status === "running").length,
      asks: on.filter((t) => t.status === "waiting_on_you" || t.status === "error").length,
    };
  }

  /** The projects that live on a machine: its added folders and where its threads work. */
  function projectsOn(h: HostInfo): number {
    const seen = new Set<string>();
    if (h.local) for (const p of hub.projects) seen.add(p.path);
    for (const t of hub.active) {
      if ((t.host ?? hub.localHost) !== h.name) continue;
      seen.add(threadProject(t));
    }
    return seen.size;
  }

  const behind = (h: HostInfo) => {
    const v = versions[h.name] ?? h.version;
    return !h.local && h.state === "online" && !!v && !!ours && ours !== "dev" && v !== "dev" && v !== ours;
  };
  const ssh = (h: HostInfo) => (h.address.startsWith("ssh ") ? h.address.slice(4) : null);
  const canOpen = (h: HostInfo) => h.local || !!ssh(h);

  /** Work, showing only this machine's threads. */
  function work(h: HostInfo) {
    hub.workScope = null;
    hub.hostFilter = hub.hosts.length > 1 ? h.name : null;
    hub.switchRoom("work");
  }

  /** A terminal inside Hub, on the machine's page. */
  function open(h: HostInfo, what: OpenOnMachine) {
    hub.openTerminal(h.name, what);
  }

  async function newThread(h: HostInfo) {
    const home = abouts[h.name]?.home ?? (await quiet(api.folders("~", at(h)).then((l) => l.home), "folders", null));
    if (home) hub.startDraft(home, "", at(h));
    else hub.openMachine(h.name);
  }

  let reconnecting = $state<string | null>(null);
  async function reconnect(h: HostInfo) {
    reconnecting = h.name;
    try {
      await api.reconnectHost(h.name);
      hub.hosts = await api.hosts();
    } catch (e) {
      hub.fail(e);
    } finally {
      reconnecting = null;
    }
  }

  const cliName = (c: EngineCli) => c.cli[0].toUpperCase() + c.cli.slice(1);

  function moreMenu(h: HostInfo): MenuEntry[] {
    return [
      { label: "Open its page", run: () => hub.openMachine(h.name) },
      { label: "Its threads in Inbox", run: () => work(h) },
      null,
      ...(canOpen(h) ? clis.slice(1).map((c) => ({ label: `Open ${cliName(c)} there`, run: () => open(h, { what: "engine", engine: c.engine }) })) : []),
      !h.local && !!ssh(h) && { label: "Root shell", run: () => open(h, { what: "root", engine: null }) },
      behind(h) && { label: "Update hubd…", hint: "On its page", run: () => hub.openMachine(h.name) },
      null,
      ...machineKindMenu(h.name, h.local),
      null,
      { label: "Machine settings", run: () => hub.openSettings("machines") },
    ];
  }
  function showMore(e: MouseEvent, h: HostInfo) {
    const r = (e.currentTarget as HTMLElement).getBoundingClientRect();
    contextMenu.show(r.left, r.bottom + 4, moreMenu(h));
  }
</script>

<Page title="Machines" width="wide">
  {#snippet head()}
    <span class="grow"></span>
    <button class="add" onclick={() => hub.openSettings("machines")}><Icon name="plus" size={14} />Add a machine</button>
  {/snippet}

  <div class="graphs">
    <section class="gr">
      <div class="rw">
        <div class="lb">Memory in use</div>
        <div class="rng">
          {#each Object.keys(RANGES) as r (r)}<button class:on={range === r} onclick={() => (range = r as Range)}>{RANGES[r as Range].label}</button>{/each}
        </div>
      </div>
      <div class="big">{bytes(total.used)}<small>of {bytes(total.mem)} on {total.count} {total.count === 1 ? "machine" : "machines"}</small></div>
      <div class="chart"><Wave values={netMem} /></div>
      <div class="ax"><span>{RANGES[range].label} ago</span><span>now</span></div>
    </section>
    <section class="gr">
      <div class="rw"><div class="lb">CPU busy</div></div>
      <div class="big">{total.busy.toFixed(1)}<small>of {total.cores} cores</small></div>
      <div class="chart"><Wave values={netCpu} warn={0.9} bad={2} tone="blue" /></div>
      <div class="ax"><span>{RANGES[range].label} ago</span><span>now</span></div>
    </section>
  </div>

  {#if room}
    <p class="room" data-tip="Free memory, less a tenth kept back, divided by what an agent takes now ({bytes(room.each)})">
      Room for about <b>{room.n}</b> more {room.n === 1 ? "agent" : "agents"}{room.best && total.count > 1 && room.n ? `, most of it on ${room.best}` : ""}.
    </p>
  {/if}

  <div class="grid">
    {#each machines as h (h.name)}
      {@const t = threadsOn(h)}
      {@const on = h.local || h.state === "online"}
      <div class="slot" use:menu={() => moreMenu(h)}>
        <MachineCard
          name={h.name}
          local={h.local}
          {h}
          snap={snaps[h.name] ?? null}
          about={abouts[h.name] ?? null}
          say={verdict(signals({ h, online: on, snap: snaps[h.name] ?? null, about: abouts[h.name] ?? null, ready: readies[h.name] ?? null, behind: behind(h), asks: t.asks, working: t.working }))}
          behind={behind(h)}
          version={versions[h.name] ?? h.version}
          mem={machineStats.series(h.name, "1h", "mem")}
          cpu={machineStats.series(h.name, "1h", "cpu")}
          rows={sessions.sessions.filter((s) => s.machine === h.name)}
          onopen={() => hub.openMachine(h.name)}
        >
          {#snippet actions()}
            {#if on}
              <button class="btn" data-tip="New thread in its home folder" onclick={() => newThread(h)}><Icon name="plus" size={13} />Thread</button>
              {#if canOpen(h)}
                <button class="btn" onclick={() => open(h, { what: "shell", engine: null })}><Icon name="terminal" size={13} />Terminal</button>
                {#if clis[0]}<button class="btn" data-tip="{clis[0].cli} in the home folder there" onclick={() => open(h, { what: "engine", engine: clis[0].engine })}><Icon name="spark" size={13} />{cliName(clis[0])}</button>{/if}
              {/if}
            {:else if !h.local}
              <button class="btn" disabled={reconnecting === h.name} onclick={() => reconnect(h)}><Icon name="restart" size={13} />{reconnecting === h.name ? "Connecting…" : "Reconnect"}</button>
            {/if}
            <button class="btn more" aria-label="More" data-tip="More" onclick={(e) => showMore(e, h)}><Icon name="more" size={14} /></button>
          {/snippet}
        </MachineCard>
      </div>
    {/each}
    <button class="new" onclick={() => hub.openSettings("machines")}>
      <Icon name="plus" size={16} />
      <span>Add a machine</span>
    </button>
  </div>
</Page>

<style>
  .grow { flex: 1; }
  .add { display: inline-flex; align-items: center; gap: 6px; color: var(--text-2); font-size: 13px; padding: 4px 10px; border-radius: var(--r-md); box-shadow: inset 0 0 0 1px var(--line-strong); }
  .add:hover { background: var(--surface); color: var(--text); }

  .graphs { display: grid; grid-template-columns: repeat(2, minmax(0, 1fr)); gap: 12px; margin-top: 18px; }
  @media (max-width: 900px) { .graphs { grid-template-columns: minmax(0, 1fr); } }
  .gr { background: var(--surface); border: 1px solid var(--line); border-radius: var(--r-lg); padding: 12px 16px 10px; min-width: 0; }
  .rw { display: flex; align-items: center; min-height: 22px; }
  .lb { color: var(--muted); font-size: 12px; }
  .rng { margin-left: auto; display: flex; gap: 2px; font-size: 11px; }
  .rng button { padding: 2px 8px; border-radius: var(--r-sm); color: var(--muted); }
  .rng button:hover { color: var(--text-2); }
  .rng button.on { background: var(--surface-hover); color: var(--text-2); }
  .big { font-size: 26px; color: var(--text); font-weight: 500; letter-spacing: -0.02em; white-space: nowrap; overflow: hidden; text-overflow: ellipsis; font-variant-numeric: tabular-nums; }
  .big small { font-size: 13px; color: var(--muted); font-weight: 400; margin-left: 8px; letter-spacing: 0; }
  .chart { height: 104px; margin-top: 8px; }
  .ax { display: flex; justify-content: space-between; color: var(--muted); font-size: 11px; margin-top: 4px; }
  .room { color: var(--text-3); font-size: 13px; margin: 14px 2px 0; }
  .room b { color: var(--text); font-weight: 500; }

  .grid { display: grid; grid-template-columns: repeat(auto-fill, minmax(calc(360px * var(--text-grow)), 1fr)); gap: 12px; margin-top: 16px; align-items: stretch; }
  .slot { display: flex; flex-direction: column; min-width: 0; }
  .slot > :global(.card) { flex: 1; }
  .new { border: 1px dashed var(--line-strong); border-radius: var(--r-lg); display: flex; flex-direction: column; align-items: center; justify-content: center; gap: 6px; color: var(--text-3); font-size: 13px; min-height: 180px; }
  .new:hover { color: var(--text); border-color: var(--text-3); }
</style>
