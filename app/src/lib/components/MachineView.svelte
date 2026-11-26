<!-- One machine's own page, in the same look as the Machines page. Left: the machine's card with what is free, what can be done there, whether it is ready for work, its recent threads and projects, and what it is. Right: two waves for its memory and CPU, then everything that runs there in sections that fold (agent sessions, services, builds, containers), and tabs for its threads, files, ports and terminals. Adding and removing machines stay in Settings.
     Used by App.svelte. -->
<script lang="ts">
  import { quiet } from "../quiet";
  import { poll } from "../poll";
  import Quiet from "./Quiet.svelte";
  import Tabs from "./Tabs.svelte";
  import PushGap from "./PushGap.svelte";
  import { onMount, untrack } from "svelte";
  import { api } from "../api";
  import type { EngineCli } from "../bindings/EngineCli";
  import type { FileText } from "../bindings/FileText";
  import type { HostInfo } from "../bindings/HostInfo";
  import type { HostSnapshot } from "../bindings/HostSnapshot";
  import type { GitIdentity } from "../bindings/GitIdentity";
  import type { LoginJob } from "../bindings/LoginJob";
  import type { MachineAbout } from "../bindings/MachineAbout";
  import type { MachineReady } from "../bindings/MachineReady";
  import type { ReadyCheck } from "../bindings/ReadyCheck";
  import type { ReadyFix } from "../bindings/ReadyFix";
  import type { CredentialCheck } from "../bindings/CredentialCheck";
  import type { CredentialState } from "../bindings/CredentialState";
  import type { OpenOnMachine } from "../bindings/OpenOnMachine";
  import type { UpdateStatus } from "../bindings/UpdateStatus";
  import type { Workers } from "../bindings/Workers";
  import type { WorkerProcess } from "../bindings/WorkerProcess";
  import type { TerminalSession } from "../bindings/TerminalSession";
  import { ago, bytes, folderName, shortVersion, threadProject } from "../format";
  import { GLYPHS } from "../refs";
  import { contextMenu, copy, menu } from "../menu.svelte";
  import { looks, MACHINE_KINDS } from "../looks.svelte";
  import { machineKindMenu } from "../menus.svelte";
  import { openUrl } from "../native";
  import { BRAND_RGB } from "../cardSwitch";
  import { LOGOS } from "../providerLogos";
  import StackLogo from "./StackLogo.svelte";
  import { scale } from "svelte/transition";
  import { hubdHas } from "../compat";
  import { stageOf, type UpdateControl } from "../machineUpdate";
  import { hub } from "../state.svelte";
  import CodeView from "./CodeView.svelte";
  import FileTree from "./FileTree.svelte";
  import Icon from "./Icon.svelte";
  import Ref from "./Ref.svelte";
  import MachineCard from "./MachineCard.svelte";
  import MachineGroup from "./MachineGroup.svelte";
  import MachineRow from "./MachineRow.svelte";
  import ReadyGroup from "./ReadyGroup.svelte";
  import RunningGroup from "./RunningGroup.svelte";
  import SessionRow from "./SessionRow.svelte";
  import Wave from "./Wave.svelte";
  import TerminalPane from "./TerminalPane.svelte";
  import { middleClose } from "../middleClose";
  import { diskLevel, memLevel, signals, tempLevel, verdict, worst } from "../machineHealth";
  import { machineStats, RANGES, type Range } from "../machineStats.svelte";
  import { sessions } from "../sessions.svelte";

  // One machine's own page, in the same look as the Machines page. Left: the machine's card with
  // what is free, what can be done there, whether it is ready for work, its recent threads and
  // projects, and what it is. Right: two waves for its memory and CPU, then everything that runs
  // there in sections that fold (agent sessions, services, builds, containers), and tabs for its
  // threads, files, ports and terminals. Adding and removing machines stay in Settings.
  let { name }: { name: string } = $props();

  const h = $derived<HostInfo | undefined>(hub.hostInfo(name));
  const local = $derived(h?.local ?? name === hub.localHost);
  const online = $derived(local || h?.state === "online");
  /** `?host=` for requests about this machine; null for this one. */
  const at = $derived(local ? null : name);

  const snap = $derived<HostSnapshot | null>(machineStats.snaps[name] ?? null);
  let about = $state<MachineAbout | null>(null);
  /** The machine's hubd is too old to say what it is. */
  let aboutMissing = $state(false);
  let home = $state<string | null>(null);
  let clis = $state<EngineCli[]>([]);

  /** Everything Hub has running there: threads, helpers, builds and containers. */
  let work = $state<Workers | null>(null);
  async function readWork() {
    if (!online) return;
    work = await api.workers(at).catch(() => work);
  }

  async function readAbout() {
    if (!online) return;
    try {
      about = await api.hostAbout(at);
      aboutMissing = false;
      home ??= about.home;
    } catch {
      aboutMissing = true;
    }
    if (!home) home = await quiet(api.folders("~", at).then((l) => l.home), "folders", null);
  }

  // Ready for work: engines found and signed in, git knowing who you are, room on the disk.
  let ready = $state<MachineReady | null>(null);
  /** The machine's hubd is too old to say. */
  let readyMissing = $state(false);
  /** Who git commits as on this computer, offered to a machine that has no one. */
  let mine = $state<GitIdentity | null>(null);
  let checking = $state(false);
  async function readReady() {
    if (!online) return;
    checking = true;
    try {
      ready = await api.machineReady(at);
      readyMissing = false;
    } catch {
      readyMissing = true;
    } finally {
      checking = false;
    }
    if (!local && !mine && ready?.checks.some((c) => c.fix?.kind === "git_identity")) {
      mine = (await quiet(api.machineReady(null), "machine ready", null))?.git ?? null;
    }
  }
  const unready = $derived(ready?.checks.filter((c) => c.state !== "ok") ?? []);
  onMount(() => {
    // Back from a terminal where something was signed in or installed: look again.
    const back = () => readReady();
    window.addEventListener("focus", back);
    quiet(api.engineClis().then((c) => (clis = c)), "engine clis");
    if (!hub.tailnet) hub.refreshTailnet();
    quiet(api.version().then((v) => (ours = v.version)), "version");
    // Only this machine is asked every few seconds, so its waves and lists move while the page is open.
    const stops = [machineStats.watch(3000, () => name), sessions.watch(3000)];
    const health = poll(() => {
      readWork();
      readUpdate();
    }, 3000);
    const slow = poll(readAbout, 60_000);
    return () => {
      window.removeEventListener("focus", back);
      stops.forEach((stop) => stop());
      health();
      slow();
    };
  });

  // Again when it comes online, or another machine's page opens in its place.
  $effect(() => {
    void name;
    if (!online) return;
    work = null;
    void machineStats.pull(name);
    readWork();
    readAbout();
    readUpdate();
    ready = null;
    readReady();
  });


  const threads = $derived(hub.active.filter((t) => (local ? !t.host || t.host === name : t.host === name)));
  const projects = $derived.by(() => {
    const seen = new Map<string, string>();
    if (local) for (const p of hub.projects) seen.set(p.path, p.name);
    for (const t of threads) {
      const cwd = threadProject(t);
      if (!seen.has(cwd)) seen.set(cwd, folderName(cwd));
    }
    return [...seen].map(([cwd, label]) => ({ cwd, label }));
  });

  const ssh = $derived(h?.address.startsWith("ssh ") ? h.address.slice(4) : null);
  /** Off the tailnet while this computer is on it: Tailscale can be put there over SSH. */
  const offTailnet = $derived(
    !local && !!ssh && !!hub.tailnet?.running && !hub.tailnet.peers.some((p) => p.machine === name),
  );

  // Terminals inside Hub on this machine (`pty.rs`): they keep running while another page shows,
  // and the Terminal tab shows the one picked.
  let terms = $state<TerminalSession[]>([]);
  async function readTerms() {
    const all = await quiet(api.terminals(), "terminals", null);
    if (all) terms = all.filter((t) => t.machine === name);
  }
  async function open(what: OpenOnMachine) {
    await hub.openTerminal(name, what);
    await readTerms();
  }
  async function closeTerm(id: string) {
    await quiet(api.closeTerminal(id), "close terminal");
    const i = terms.findIndex((t) => t.id === id);
    terms = terms.filter((t) => t.id !== id);
    if (hub.machineTerminal === id) hub.machineTerminal = terms[Math.min(i, terms.length - 1)]?.id ?? null;
    if (!terms.length && tab === "terminal") tab = "overview";
  }
  function ended(id: string, code: number) {
    terms = terms.map((t) => (t.id === id ? { ...t, exit: code } : t));
    // Something may have been signed in or installed there.
    readReady();
  }
  const term = $derived(terms.find((t) => t.id === hub.machineTerminal) ?? null);
  // Another page asked for a terminal here: show it.
  // Once per terminal, and only when it is on this machine: another machine's terminal left in
  // `hub.machineTerminal` used to pull this page to an empty Terminal tab again each time the list
  // was read, so the other tabs flashed and vanished.
  let jumpedTo: string | null = null;
  let askedFor: string | null = null;
  $effect(() => {
    const id = hub.machineTerminal;
    if (!id) {
      jumpedTo = askedFor = null;
      return;
    }
    if (id === jumpedTo) return;
    if (terms.some((t) => t.id === id)) {
      jumpedTo = id;
      untrack(() => (tab = "terminal"));
    } else if (id !== askedFor) {
      // Read the list once for it; if it is on another machine it never turns up here.
      askedFor = id;
      untrack(readTerms);
    }
  });
  // No Terminal tab here, so none is shown.
  $effect(() => {
    if (tab === "terminal" && !terms.length) tab = "overview";
  });

  /** The same in a window of its own, for when one is wanted. Says what to type when none opens. */
  let opened = $state<string | null>(null);
  async function popOut(t: TerminalSession) {
    try {
      const r = await api.openOnMachine(name, { what: t.what, engine: engineOf(t), ...(t.admin ? { admin: true } : {}) });
      opened = r.opened ? null : r.attach;
    } catch (e) {
      hub.fail(e);
    }
  }
  /** The engine a terminal runs the CLI of, read back from its title ("claude on tincan"). */
  function engineOf(t: TerminalSession): string | null {
    if (t.what !== "engine" && t.what !== "sign_in") return null;
    const word = t.what === "sign_in" ? t.title.split(" ")[1] : t.title.split(" ")[0];
    return t.what === "sign_in" ? word : (clis.find((c) => c.cli === word)?.engine ?? null);
  }

  let busy = $state<string | null>(null);
  let sudo = $state<string | null>(null);
  let password = $state("");
  async function act(label: string, run: () => Promise<unknown>) {
    busy = label;
    try {
      await run();
      hub.hosts = await api.hosts();
    } catch (e) {
      hub.fail(e);
    } finally {
      busy = null;
    }
  }
  const joinTailnet = (pw?: string) =>
    act("tailnet", async () => {
      const reply = await api.joinTailnet(name, pw ? { password: pw } : {});
      sudo = reply.sudo;
      password = "";
    });
  // Updating its hubd: what its own updater says, read every few seconds, so the page shows the
  // update from the click to the switch instead of a button that only blinks.
  let upd = $state<UpdateStatus | null>(null);
  let ours = $state<string | null>(null);
  /** Clicked Update and not yet seen it end: the version it ran then. */
  let updatingFrom = $state<string | null>(null);
  let updatedTo = $state<string | null>(null);
  /** The green "updated" shows a few seconds, then the card shows the version again. */
  let justUpdated = $state(false);
  async function readUpdate() {
    if (local || !online) return;
    const now = await quiet(api.version(at), "version", null);
    if (!now) return;
    upd = now;
    if (updatingFrom !== null && now.version !== updatingFrom) {
      updatedTo = now.version;
      justUpdated = true;
      setTimeout(() => (justUpdated = false), 6000);
      updatingFrom = null;
      readAbout();
      readReady();
    } else if (updatingFrom !== null && now.state === "failed") updatingFrom = null;
  }
  const behind = $derived(!!upd && !!ours && ours !== "dev" && upd.version !== ours);
  /** What stays as a line under the card: the failure's reason, and a built version that waits to switch. The rest is in the card's update control. */
  const updateLine = $derived.by(() => {
    if (local || !online) return null;
    const u = upd;
    const target = u?.target ? ` ${shortVersion(u.target)}` : "";
    if (u?.state === "ready") return { text: `hubd${target} is built on ${name}. It switches once no thread there is working.`, ready: true };
    if (u?.state === "failed") return { text: `The update on ${name} failed${u.message ? `: ${u.message}` : "."}`, failed: true };
    if (updatingFrom !== null && u?.state === "disabled") return { text: `${name} has updates switched off, so it does not take the new version. Install hubd again from Settings > Machines.`, failed: true };
    return null;
  });
  /** How far the update has got, for the card's top right. */
  const stage = $derived(local || !online ? null : stageOf({ name, upd, sending: h?.sending, sent: updatingFrom !== null, updatedTo: justUpdated ? updatedTo : null }));
  /** The furthest dot reached in this update, so the control never goes back a step. */
  let reached = $state(0);
  $effect(() => {
    if (!stage) reached = 0;
    else if (stage.step !== "failed" && stage.dots > reached) reached = stage.dots;
  });
  const control = $derived.by<UpdateControl | null>(() => {
    if (local || !online || !ssh) return null;
    const busyNow = busy === "update";
    if (stage) {
      const failed = stage.step === "failed";
      const done = stage.step === "updated";
      return { step: stage.step, dots: failed ? reached : done ? 5 : Math.max(reached, stage.dots), tone: stage.tone, button: failed ? "Try again" : null, busy: busyNow, tip: stage.tip, run: update };
    }
    if (!behind) return null;
    return { step: null, dots: 0, tone: "work", button: "Update", busy: busyNow, tip: `Send hubd ${shortVersion(ours)} from this computer to ${name} (it runs ${shortVersion(upd?.version)})`, run: update };
  });
  const update = () =>
    act("update", async () => {
      updatedTo = null;
      justUpdated = false;
      const reply = await api.updateHost(name);
      // A password or a build choice is asked where installing is: in Settings.
      if (reply.sudo || reply.build) hub.openSettings("machines");
      else {
        updatingFrom = upd?.version ?? h?.version ?? "";
        readUpdate();
      }
    });
  const switchNow = () => act("switch", () => api.installUpdate(at));

  let file = $state<FileText | null>(null);
  async function openFile(rel: string) {
    if (!home) return;
    try {
      file = await api.projectFile(home, rel, at);
    } catch (e) {
      hub.fail(e);
    }
  }

  function uptime(secs: number): string {
    const d = Math.floor(secs / 86400);
    const hrs = Math.floor((secs % 86400) / 3600);
    if (d) return `${d} ${d === 1 ? "day" : "days"}${hrs ? `, ${hrs} h` : ""}`;
    const m = Math.floor((secs % 3600) / 60);
    return hrs ? `${hrs} h ${m} min` : `${m} min`;
  }
  // The card on the left and its one line, from the same rules as the Machines page.
  const asks = $derived(threads.filter((t) => t.status === "waiting_on_you" || t.status === "error").length);
  const working = $derived(threads.filter((t) => t.status === "running").length);
  const say = $derived(verdict(signals({ h, online, snap, about, ready, behind, asks, working })));
  const ports = $derived(snap ? snap.listeners.filter((l) => !l.system) : []);

  let range = $state<Range>("1h");
  const memSeries = $derived(machineStats.series(name, range, "mem"));
  const cpuSeries = $derived(machineStats.series(name, range, "cpu"));
  const memL = $derived(snap ? memLevel(snap.mem_used, snap.mem_total) : "ok");
  const GB = 2 ** 30;

  /** The agent sessions running here, with the sleeping ones last. */
  const here = $derived(sessions.sessions.filter((s) => s.machine === name));
  const hereMem = $derived(here.reduce((n, s) => n + s.mem, 0));
  const services = $derived(snap ? snap.listeners.filter((l) => !l.system) : []);
  /** Every process Hub knows of, by pid, for the memory and CPU of a service. */
  const procs = $derived(new Map<number, WorkerProcess>([...(work?.threads ?? []).flatMap((w) => w.processes), ...(work?.hub ?? [])].map((p) => [p.pid, p])));
  const servicesMem = $derived(services.reduce((n, l) => n + (l.pid != null ? (procs.get(l.pid)?.mem ?? 0) : 0), 0));
  /** Hub's own processes as one line each: a process at the top and what it started summed in. */
  const roots = $derived.by(() => {
    const out: { p: WorkerProcess; mem: number; cpu: number; more: number }[] = [];
    for (const p of work?.hub ?? []) {
      const last = out[out.length - 1];
      if (p.depth > 0 && last) {
        last.mem += p.mem;
        last.cpu += p.cpu_percent;
        last.more++;
      } else out.push({ p, mem: p.mem, cpu: p.cpu_percent, more: 0 });
    }
    return out;
  });
  const hubMem = $derived(roots.reduce((n, r) => n + r.mem, 0));
  /** What Hub does not account for: the machine's memory less sessions, helpers and containers. */
  const rest = $derived(snap ? Math.max(0, snap.mem_used - (work?.threads ?? []).reduce((n, w) => n + w.mem, 0) - hubMem) : 0);

  /** The trouble on this machine as one bar, when there is any: the worst of what the card says. */
  const alert = $derived.by(() => {
    if (!snap || say.level !== "bad") return null;
    const l = worst(memL, about?.disk ? diskLevel(about.disk.free, about.disk.total) : "ok", say.level);
    const big = [...here.map((s) => ({ t: s.thread?.title ?? "A removed thread", mem: s.mem })), ...roots.map((r) => ({ t: r.p.name === roots[0]?.p.name ? "Hub" : r.p.name, mem: r.mem }))].sort((a, b) => b.mem - a.mem)[0];
    const detail = memL !== "ok" && big && big.mem > 0.2 * GB ? `The most memory: ${big.t}, ${bytes(big.mem)}` : "";
    return { level: l, title: say.text, detail };
  });

  async function stopPid(pid: number) {
    try {
      await api.stopProcess(pid, at);
      await Promise.all([readWork(), machineStats.pull(name)]);
    } catch (e) {
      hub.fail(e);
    }
  }

  type Tab = "overview" | "threads" | "files" | "ports" | "projects" | "terminal";
  let tab = $state<Tab>("overview");
  $effect(() => {
    void name;
    // Another machine's terminals are not this one's: start empty and read this machine's.
    terms = [];
    jumpedTo = askedFor = null;
    tab = "overview";
    file = null;
    untrack(readTerms);
  });
  // The overview's groups sit in columns of their own: how many follows the width, and which group
  // goes in which column is fixed, so folding one moves nothing in another column.
  const ALL_RIGHT = true;
  let groupsW = $state(1500);
  const memTotal = $derived(snap?.mem_total ?? 0);
  const rg = $derived({ name, snap, work, services, servicesMem, procs, roots, hubMem, memTotal, rest, stopPid });
  const lanes = $derived.by(() => {
    const n = groupsW >= 1480 ? 3 : groupsW >= 980 ? 2 : 1;
    const all = ALL_RIGHT;
    const ready = all ? ["ready"] : [];
    if (n === 3) return [["sessions", "else", "recent"], ["services", "containers"], ["builds", ...ready, "projects"]];
    if (n === 2) return [["sessions", "builds", "else", "recent"], ["services", "containers", ...ready, "projects"]];
    return [["sessions", "services", "builds", "containers", "else", ...ready, "recent", "projects"]];
  });
  const cliName = (c: EngineCli) => c.cli[0].toUpperCase() + c.cli.slice(1);
  /** An engine's own colours on its button, the same two as its picker and limits card. */
  const tint = (engine: string) => (BRAND_RGB[engine] ? `--b: ${BRAND_RGB[engine].a}; --b2: ${BRAND_RGB[engine].b}` : undefined);
  // An administrator window only on this computer when it is Windows: a remote Windows machine's
  // SSH shell is already elevated for an administrator, and Linux and Mac have the Root shell.
  const localWindows = $derived(local && /windows/i.test(about?.os ?? snap?.os ?? ""));
  let adminOk = $state(false);
  quiet(hubdHas("machine.admin").then((v) => (adminOk = v)), "hubd has"); // compat-until: 2026-12-03
  // A newer hubd runs the elevated shell inside Hub's Terminal tab; an older one opens a window.
  let adminInHub = $state(false);
  quiet(hubdHas("machine.admin.terminal").then((v) => (adminInHub = v)), "hubd has"); // compat-until: 2026-12-03
  /** Admin is switched on: the next Terminal, Claude or Codex press opens elevated. */
  let adminOn = $state(false);
  $effect(() => {
    name;
    adminOn = false;
  });
  const go = (what: OpenOnMachine) => (adminOn && localWindows ? (adminInHub ? open({ ...what, admin: true }) : asAdmin(what)) : open(what));
  async function asAdmin(what: OpenOnMachine) {
    try {
      await api.openOnMachine(name, { ...what, admin: true });
    } catch (e) {
      hub.fail(e);
    }
  }
</script>

<div class="page page-fill">
  <button class="back" onclick={() => { hub.view = "machines"; hub.selected = null; }}><Icon name="chevron" size={12} />Machines</button>

  <div class="layout">
    <!-- Left: the machine as on the Machines page, what can be done there, and what it is. -->
    <aside class="left">
      <MachineCard {name} {local} {h} {snap} {about} {say} {behind} update={control} side version={upd?.version ?? h?.version ?? null} threads={threads.length} projects={projects.length} mem={machineStats.series(name, "1h", "mem")} cpu={machineStats.series(name, "1h", "cpu")} />

      <div class="kindrow">
        <button class="tag kind" data-tip="Change its icon" use:menu={() => machineKindMenu(name, local)} onclick={(e) => { const r = e.currentTarget.getBoundingClientRect(); contextMenu.show(r.left, r.bottom + 4, machineKindMenu(name, local)); }}>
          {MACHINE_KINDS.find((k) => k.kind === looks.machineKind(name, local))?.label}<Icon name="down" size={10} />
        </button>
        <span class="grow"></span>
        {#if h?.version}<span class="ver" data-tip="hubd version">hubd {shortVersion(h.version)}</span>{/if}
        <button class="act ghost" data-tip="Machines and SSH in Settings" onclick={() => hub.openSettings("machines")}><Icon name="gear" size={14} /></button>
      </div>

      <section class="open">
        {#if online && home}
          <button class="btn primary wide" onclick={() => hub.startDraft(home!, "", local ? null : name)}><Icon name="plus" size={14} />New thread here</button>
        {/if}
        {#if local || ssh}
          <div class="pair">
            <button class="btn" onclick={() => go({ what: "shell", engine: null })}><Icon name="terminal" size={14} />Terminal{@render sh()}</button>
            {#if !local}
              <button class="btn" data-tip="sudo -i there; asks for its password in the terminal" onclick={() => open({ what: "root", engine: null })}><Icon name="shield" size={14} />Root shell</button>
            {:else}
              {#each clis as c (c.engine)}
                <button class="btn eng" style={tint(c.engine)} data-tip="{c.cli} in the home folder here" onclick={() => go({ what: "engine", engine: c.engine })}>{@render mark(c.engine)}{cliName(c)}{@render sh()}</button>
              {/each}
              {#if localWindows}
                <!-- One press, then Terminal, Claude or Codex opens elevated: Windows asks (UAC) and the terminal opens in the Terminal tab (a window of its own on an older hubd; compat-until: 2026-12-03). -->
                <button class="btn adm-t" class:on={adminOn} aria-pressed={adminOn} disabled={!adminOk} data-tip={adminOk ? "Open the next terminal as administrator" : "Needs the next version of Hub"} onclick={() => (adminOn = !adminOn)}><Icon name="shield" size={14} />Admin</button>
              {/if}
            {/if}
          </div>
          {#if !local && clis.length}
            <div class="pair">
              {#each clis as c (c.engine)}
                <button class="btn eng" style={tint(c.engine)} data-tip="{c.cli} in the home folder there" onclick={() => open({ what: "engine", engine: c.engine })}>{@render mark(c.engine)}{cliName(c)}</button>
              {/each}
            </div>
          {/if}
        {/if}
        {#if !local && h?.state !== "online"}
          <button class="btn wide" disabled={busy === "reconnect"} onclick={() => act("reconnect", () => api.reconnectHost(name))}><Icon name="restart" size={14} />{busy === "reconnect" ? "Connecting…" : "Reconnect"}</button>
        {/if}
        {#if offTailnet && (!h?.join || h.join.failed) && !sudo}
          <button class="btn wide" disabled={busy === "tailnet"} data-tip="Installs Tailscale there over SSH and gives you the link to sign it in" onclick={() => joinTailnet()}><Icon name="globe" size={14} />{busy === "tailnet" ? "Starting…" : "Add to tailnet"}</button>
        {/if}
      </section>

      {#if updateLine}
        <p class="note update" class:failed={updateLine.failed}>
          <span>{updateLine.text}</span>
          {#if updateLine.ready}<button class="link" disabled={busy === "switch"} onclick={switchNow}>Switch now</button>{/if}
        </p>
      {/if}
      {#if h?.error}<p class="note failed">{h.error}</p>{/if}
      {#if !local && !ssh && h}<p class="note">Reached by URL only. Add an SSH alias for a terminal.</p>{/if}
      {#if !local && ssh && h?.removable}<PushGap {name} {h} />{/if}
      {#if sudo}
        <form class="note sudo" onsubmit={(e) => { e.preventDefault(); joinTailnet(password); }}>
          <span>{sudo} The password goes to sudo there and is not kept.</span>
          <input type="password" bind:value={password} placeholder="sudo password" autocomplete="off" />
          <button class="btn" disabled={!password || busy === "tailnet"}>Add to tailnet</button>
          <button class="btn" type="button" onclick={() => { sudo = null; password = ""; }}>Cancel</button>
        </form>
      {/if}
      {#if h?.join}
        <p class="note">
          Tailscale: {h.join.step}{h.join.failed ? "" : "…"}
          {#if h.join.login_url}<button class="link" onclick={() => openUrl(h!.join!.login_url!).catch((e) => hub.fail(e))}>Sign {name} in to your tailnet</button>{/if}
        </p>
      {/if}

      {#if about}
        <dl class="about">
          {#if about.model}<dt>Board</dt><dd>{about.model}</dd>{/if}
          <dt>System</dt><dd>{about.os}{#if about.kernel}{` · kernel ${about.kernel}`}{/if}</dd>
          <dt>CPU</dt><dd>{[about.cpu, `${about.cores} ${about.cores === 1 ? "core" : "cores"}`, about.arch].filter(Boolean).join(" · ")}</dd>
          <dt>Up for</dt><dd>{uptime(about.uptime_secs)}</dd>
          {#if about.temp_c != null}<dt>Temperature</dt><dd class={tempLevel(about.temp_c, about)}>{Math.round(about.temp_c)} °C</dd>{/if}
          <dt>Tailscale</dt><dd>{about.tailscale}</dd>
          {#if !local && h?.address}<dt>Address</dt><dd>{h.address}</dd>{/if}
          <dt>Home</dt><dd class="mono">{about.home}</dd>
        </dl>
      {:else if aboutMissing && online}
        <p class="muted about">Needs a newer hubd.</p>
      {/if}

      {#if online && !ALL_RIGHT}
        {@render readyG()}
      {/if}
    </aside>

    <!-- Right: everything else, a tab each. -->
    <div class="right">
      {#if online}
        <Tabs style="margin: 0 0 18px" value={tab} onchange={(id) => { tab = id; if (id === "terminal") hub.machineTerminal ??= terms[0].id; }} items={[
          { id: "overview", label: "Overview" },
          { id: "threads", label: "Threads", count: threads.length },
          { id: "files", label: "Files" },
          { id: "ports", label: "Ports", count: ports.length },
          { id: "projects", label: "Projects", count: projects.length },
          ...(terms.length ? [{ id: "terminal" as const, label: "Terminal", count: terms.length }] : []),
        ]} />

        {#if tab === "overview"}
          {#if !snap}
            <Quiet kind="loading" />
          {:else}
            {#if alert}
              <div class="adv {alert.level}"><i></i><b>{alert.title}</b>{#if alert.detail}<span>{alert.detail}</span>{/if}</div>
            {/if}
            <div class="graphs">
              <section class="gr">
                <div class="rw">
                  <div class="lb">Memory in use</div>
                  <div class="rng">
                    {#each Object.keys(RANGES) as r (r)}<button class:on={range === r} onclick={() => (range = r as Range)}>{RANGES[r as Range].label}</button>{/each}
                  </div>
                </div>
                <div class="big {memL}">{bytes(snap.mem_used)}<small>of {bytes(snap.mem_total)} · {bytes(Math.max(0, snap.mem_total - snap.mem_used))} free</small></div>
                <div class="chart"><Wave values={memSeries} /></div>
                <div class="ax"><span>{RANGES[range].label} ago</span><span>now</span></div>
              </section>
              <section class="gr">
                <div class="rw"><div class="lb">CPU busy</div></div>
                <div class="big">{Math.min(100, Math.round(snap.cpu_percent))} %<small>{about?.cores ? `of ${about.cores} ${about.cores === 1 ? "core" : "cores"}` : ""}</small></div>
                <div class="chart"><Wave values={cpuSeries} warn={0.9} bad={2} tone="blue" /></div>
                <div class="ax"><span>{RANGES[range].label} ago</span><span>now</span></div>
              </section>
            </div>

            <div class="groups" bind:clientWidth={groupsW}>
              {#each lanes as lane, i (i)}
                <div class="lane">
                  {#each lane as id (id)}
                    {#if id === "sessions"}{@render sessionsG()}
                    {:else if id === "services"}{@render servicesG()}
                    {:else if id === "builds"}{@render buildsG()}
                    {:else if id === "containers"}{@render containersG()}
                    {:else if id === "else"}{@render elseG()}
                    {:else if id === "ready"}{@render readyG()}
                    {:else if id === "recent"}{@render recentG()}
                    {:else if id === "projects"}{@render projectsG()}
                    {/if}
                  {/each}
                </div>
              {/each}
            </div>
          {/if}
        {:else if tab === "threads"}
          {@render threadList(threads.length)}
        {:else if tab === "files"}
          <p class="muted path mono">{home ?? ""}</p>
          {#if home}
            <div class="browse">
              <div class="tree">{#key `${name}\u0000${home}`}<FileTree path={home} host={at} selected={file?.rel ?? null} onopen={openFile} />{/key}</div>
              <div class="file">
                {#if file}<CodeView path={file.rel} text={file.text} />{:else}<p class="muted">Pick a file.</p>{/if}
              </div>
            </div>
          {:else}
            <Quiet kind="loading" />
          {/if}
        {:else if tab === "ports"}
          {#if ports.length}
            <div class="ports">
              {#each ports as l (`${l.port}-${l.pid}-${l.protocol}`)}
                <span class="mono">:{l.port}</span><span class="proc" data-tip={l.command ?? l.exe ?? ""}>{l.process ?? "?"}</span><span class="muted">{l.is_hub ? "Hub" : l.thread_id ? "a thread" : ""}</span>
              {/each}
            </div>
          {:else}
            <Quiet text="Nothing listening" />
          {/if}
          {#if local}<button class="link" onclick={() => { hub.view = "services"; hub.selected = null; }}>Open Services</button>{/if}
        {:else if tab === "projects"}
          {#if projects.length}{@render projectList()}{:else}<Quiet text="No projects here" />{/if}
        {:else if tab === "terminal"}
          <div class="terms">
            {#each terms as t (t.id)}
              <span class="termtab" class:on={t.id === term?.id} class:ended={t.exit != null} use:middleClose={() => closeTerm(t.id)}>
                <button class="pick" data-tip={t.command} onclick={() => (hub.machineTerminal = t.id)}><Icon name={t.admin || t.what === "root" ? "shield" : t.what === "engine" || t.what === "sign_in" ? "spark" : "terminal"} size={12} />{t.title}</button>
                <button class="x" data-tip="Close; ends what runs in it" onclick={() => closeTerm(t.id)}><Icon name="x" size={10} /></button>
              </span>
            {/each}
            <span class="grow"></span>
            {#if term}<button class="link" data-tip="Opens the same in a terminal window of its own" onclick={() => popOut(term!)}><Icon name="popout" size={12} /> Open in a window</button>{/if}
          </div>
          {#if opened}
            <p class="note">Type this in a terminal: <code>{opened}</code> <button class="link" onclick={() => copy(opened!)}>Copy</button></p>
          {/if}
          <div class="termbox">
            {#if term}
              {#key term.id}<TerminalPane id={term.id} onexit={(code) => ended(term!.id, code)} />{/key}
            {/if}
          </div>
        {/if}
      {:else if h?.state === "connecting"}
        <p class="why">Connecting…</p>
      {:else if !h}
        <p class="why">No machine called {name}.</p>
      {:else}
        <p class="why">{name} is not reachable.</p>
      {/if}
    </div>
  </div>
</div>

{#snippet readyG()}
  <ReadyGroup {name} {at} {online} {local} {ssh} {ready} {readyMissing} {checking} {mine} onread={readReady} onopen={open} />
{/snippet}
{#snippet recentG()}
  <MachineGroup machine={name} id="recent" title="Recent threads" count={threads.length}>
    <div class="lines">{@render threadList(6)}</div>
  </MachineGroup>
{/snippet}
{#snippet projectsG()}
  {#if projects.length}
    <MachineGroup machine={name} id="projects" title="Projects here" count={projects.length} open={false}>
      <div class="lines">{@render projectList()}</div>
    </MachineGroup>
  {/if}
{/snippet}
{#snippet sessionsG()}
  {#if snap}
  <MachineGroup machine={name} id="sessions" title="Sessions" count={here.length} mem={hereMem} total={memTotal}>
    {#snippet icon()}<Icon name="threads" size={15} />{/snippet}
    {#each here as s (`${s.host ?? ""}|${s.id}`)}<SessionRow {s} total={memTotal} wide />{:else}<Quiet text="No agent session running" />{/each}
  </MachineGroup>
  {/if}
{/snippet}
{#snippet servicesG()}
  <RunningGroup kind="services" {...rg} />
{/snippet}
{#snippet buildsG()}
  <RunningGroup kind="builds" {...rg} />
{/snippet}
{#snippet containersG()}
  <RunningGroup kind="containers" {...rg} />
{/snippet}
{#snippet elseG()}
  <RunningGroup kind="else" {...rg} />
{/snippet}
{#snippet mark(engine: string)}
  {#if engine === "claude" || engine === "codex"}
    <svg width="14" height="14" viewBox="0 0 24 24" fill="currentColor" aria-hidden="true"><path d={LOGOS[engine === "claude" ? "claude" : "openai"].path} /></svg>
  {:else}
    <Icon name="spark" size={14} />
  {/if}
{/snippet}

{#snippet sh()}
  {#if adminOn}<span class="sh" transition:scale={{ duration: 160, start: 0.4 }}><Icon name="shield" size={9} /></span>{/if}
{/snippet}

{#snippet threadList(n: number)}
  {#each threads.slice(0, n) as t (t.id)}
    <button class="thread" onclick={() => hub.open(t.id)}>
      <i class="dot {t.status}"></i><span class="title">{t.title}</span><span class="when">{ago(t.updated_at)}</span>
    </button>
  {:else}
    <Quiet kind="none" />
  {/each}
  {#if threads.length > n}<button class="link" onclick={() => (tab = "threads")}>All {threads.length}</button>{/if}
  {#if threads.length}<button class="link inbox" onclick={() => { hub.workScope = null; hub.hostFilter = hub.hosts.length > 1 ? name : null; hub.switchRoom("work"); }}>Open in Inbox</button>{/if}
{/snippet}

{#snippet projectList()}
  <p class="projects">{#each projects as p (p.cwd)}<Ref kind="project" project={{ path: p.cwd, host: local ? null : name }} label={p.label} title={p.cwd} onopen={() => hub.openProject(p.cwd, local ? null : name)} />{/each}</p>
{/snippet}

<style>
  .page { flex: 1; min-height: 0; overflow-y: auto; width: 100%; }
  .back { display: inline-flex; align-items: center; gap: 4px; color: var(--text-3); font-size: 13px; margin-bottom: 12px; }
  .back :global(svg) { transform: rotate(180deg); }
  .back:hover { color: var(--text); }
  .layout { display: grid; grid-template-columns: calc(330px * var(--text-grow)) minmax(0, 1fr); gap: 28px; align-items: start; }
  @media (max-width: 900px) { .layout { grid-template-columns: minmax(0, 1fr); } }
  .left { display: flex; flex-direction: column; gap: 12px; min-width: 0; }
  .grow { flex: 1; }
  .kindrow { display: flex; align-items: center; gap: 8px; margin-top: -2px; }
  .tag { color: var(--muted); font-size: 12px; }
  .tag.kind { display: inline-flex; align-items: center; gap: 4px; padding: 1px 6px; border-radius: var(--r-sm); }
  .tag.kind:hover { background: var(--surface-hover); color: var(--text-2); }
  .ver { color: var(--muted); font: 12px var(--mono); }
  .act.ghost { color: var(--text-3); padding: 4px; border-radius: var(--r-sm); }
  .act.ghost:hover { color: var(--text); background: var(--surface); }
  .open { display: flex; flex-direction: column; gap: 6px; }
  .open .pair { display: flex; gap: 6px; flex-wrap: wrap; }
  .open .btn { display: inline-flex; align-items: center; justify-content: center; gap: 4px; height: 30px; padding: 0 8px; font-size: 13px; white-space: nowrap; position: relative; }
  .open .pair .btn { flex: 1 0 auto; }
  .open .btn :global(svg) { flex: none; }
  .open .btn.wide { width: 100%; }
  .open .btn.eng { color: rgb(var(--b)); background: color-mix(in srgb, rgb(var(--b)) 10%, var(--surface-hover)); box-shadow: inset 0 0 0 1px rgba(var(--b), 0.28); }
  .open .btn.eng:hover:not(:disabled) { background: color-mix(in srgb, rgb(var(--b)) 17%, var(--surface-hover)); }
  .open .btn:disabled { opacity: 0.5; cursor: default; }
  /* A small shield on the corner of each button says the next press opens as administrator. */
  .open .sh { position: absolute; top: -5px; right: -4px; width: 15px; height: 15px; border-radius: 50%; display: grid; place-items: center; background: var(--accent); color: var(--bg); }
  .open .adm-t { flex: none; color: var(--text-3); }
  .open .adm-t.on { color: var(--accent); background: color-mix(in srgb, var(--accent) 18%, var(--surface-hover)); box-shadow: inset 0 0 0 1px var(--accent); }
  .open .adm-t.on :global(svg) { animation: pulse-ok 0.2s var(--ease) 1; }
  @keyframes adm-ping { 0% { box-shadow: inset 0 0 0 1px var(--accent), 0 0 0 0 color-mix(in srgb, var(--accent) 55%, transparent); } 100% { box-shadow: inset 0 0 0 1px var(--accent), 0 0 0 9px transparent; } }
  @keyframes adm-glow { 0%, 100% { box-shadow: inset 0 0 0 1px var(--accent); } 50% { box-shadow: inset 0 0 0 1px var(--accent), 0 0 10px color-mix(in srgb, var(--accent) 40%, transparent); } }
  :global(.calm) .open .adm-t.on, :global(.calm) .open .adm-t.on :global(svg) { animation: none; }
  @media (prefers-reduced-motion: reduce) { .open .adm-t.on, .open .adm-t.on :global(svg) { animation: none; } }
  .why { color: var(--text-3); font-size: 13px; margin: 4px 0 0; }
  .note { color: var(--text-3); font-size: 13px; margin: 0; display: flex; flex-wrap: wrap; align-items: center; gap: 8px; }
  .note code { font: 12px var(--mono); color: var(--text-2); background: var(--surface); padding: 1px 6px; border-radius: var(--r-sm); }
  .sudo input { height: 26px; padding: 0 8px; border-radius: var(--r-sm); background: var(--surface); color: var(--text); border: 1px solid var(--line-strong); font-size: 13px; }
  .note.failed { color: var(--red); }
  .about { display: grid; grid-template-columns: max-content 1fr; gap: 6px 14px; margin: 6px 0 0; font-size: 13px; }
  dt { color: var(--muted); }
  dd { margin: 0; color: var(--text-2); min-width: 0; overflow-wrap: anywhere; }
  dd.mono { font-size: 12px; }
  .warn { color: var(--amber); }
  .bad { color: var(--red); }

  .right { min-width: 0; }
  .lines { padding: 0 8px; }
  .none { color: var(--muted); font-size: 12px; margin: 0; padding: 4px 8px; }

  .graphs { display: grid; grid-template-columns: repeat(2, minmax(0, 1fr)); gap: 12px; margin-bottom: 18px; }
  @media (max-width: 1100px) { .graphs { grid-template-columns: minmax(0, 1fr); } }
  .gr { background: var(--surface); border: 1px solid var(--line); border-radius: var(--r-lg); padding: 12px 16px 10px; min-width: 0; }
  .rw { display: flex; align-items: center; min-height: 22px; }
  .lb { color: var(--muted); font-size: 12px; }
  .rng { margin-left: auto; display: flex; gap: 2px; font-size: 11px; }
  .rng button { padding: 2px 8px; border-radius: var(--r-sm); color: var(--muted); }
  .rng button:hover { color: var(--text-2); }
  .rng button.on { background: var(--surface-hover); color: var(--text-2); }
  .big { font-size: 26px; color: var(--text); font-weight: 500; letter-spacing: -0.02em; white-space: nowrap; overflow: hidden; text-overflow: ellipsis; font-variant-numeric: tabular-nums; }
  .big.warn { color: var(--amber); }
  .big.bad { color: var(--red); }
  .big small { font-size: 13px; color: var(--muted); font-weight: 400; margin-left: 8px; letter-spacing: 0; }
  .chart { height: 112px; margin-top: 8px; }
  .ax { display: flex; justify-content: space-between; color: var(--muted); font-size: 11px; margin-top: 4px; }
  .adv { display: flex; align-items: center; gap: 10px; border-radius: var(--r-lg); padding: 10px 12px; color: var(--text-2); margin-bottom: 12px; background: var(--amber-bg); border: 1px solid color-mix(in srgb, var(--amber) 35%, transparent); }
  .adv i { width: 7px; height: 7px; border-radius: 50%; background: var(--amber); flex: none; }
  .adv b { color: var(--amber); font-weight: 500; }
  .adv span { color: var(--text-3); }
  .adv.bad { background: var(--red-bg); border-color: color-mix(in srgb, var(--red) 35%, transparent); }
  .adv.bad i { background: var(--red); }
  .adv.bad b { color: var(--red); }
  /* Sections side by side: three columns on an ultrawide screen, two on a normal one, one when narrow. Each scrolls inside itself. */
  .groups { display: flex; gap: 28px; align-items: flex-start; }
  .lane { flex: 1 1 0; min-width: 0; display: flex; flex-direction: column; gap: 16px; }
  .muted { color: var(--muted); font-size: 13px; margin: 0; }
  .ports { display: grid; grid-template-columns: max-content max-content 1fr; gap: 6px 16px; font-size: 13px; color: var(--text-2); margin-bottom: 12px; }
  .ports .mono { font: 12px var(--mono); }
  .thread { display: flex; align-items: center; gap: 8px; width: 100%; text-align: left; padding: 4px 0; font-size: 13px; color: var(--text-2); border-bottom: 1px solid var(--line); }
  .thread:hover .title { color: var(--text); }
  .title { flex: 1; min-width: 0; white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
  .when { color: var(--muted); font-size: 12px; }
  .projects { display: flex; flex-wrap: wrap; gap: 4px 14px; margin: 0; color: var(--text-3); font-size: 13px; }
  .link { color: var(--accent); font-size: 13px; margin-top: 8px; margin-right: 12px; }
  .link:hover { text-decoration: underline; }
  .path { margin-bottom: 8px; }
  .browse { display: grid; grid-template-columns: minmax(220px, 300px) 1fr; gap: 16px; min-height: 360px; max-height: 72vh; border-top: 1px solid var(--line); padding-top: 8px; }
  .tree { overflow-y: auto; min-height: 0; }
  .file { overflow: auto; min-width: 0; min-height: 0; }
  .terms { display: flex; align-items: center; gap: 4px; flex-wrap: wrap; margin-bottom: 8px; }
  .terms .link { margin: 0; }
  .termtab { display: inline-flex; align-items: center; border-radius: var(--r-sm); color: var(--text-3); font-size: 13px; }
  .termtab:hover { background: var(--surface); color: var(--text-2); }
  .termtab.on { background: var(--surface-hover); color: var(--text); }
  .termtab.ended .pick { color: var(--muted); }
  .termtab .pick { display: inline-flex; align-items: center; gap: 6px; padding: 4px 4px 4px 8px; }
  .termtab .x { padding: 6px 8px; color: var(--muted); border-radius: var(--r-sm); }
  .termtab .x:hover { color: var(--text); }
  .termbox { height: max(360px, calc(100vh - 230px)); }
</style>
