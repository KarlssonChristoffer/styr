<!-- Two pages share the message box: Home (a board of cards about what is going on, with the box at the bottom) and New thread (nothing but the box). One component so what you typed, the folder and the machine stay when you go from one to the other.
     Used by App.svelte. -->
<script lang="ts">
  import { pop } from "../popover";
  import { untrack } from "svelte";
  import { api } from "../api";
  import type { Attachment } from "../bindings/Attachment";
  import type { Brought } from "../bindings/Brought";
  import type { SettingValue } from "../bindings/SettingValue";
  import { ago, codingFolderDefault, folderName, isScratchRun, STARTING_CHAT } from "../format";
  import { inTauri, pickFolder } from "../native";
  import { emptySession, knownOptions, modelLabel, saveChoices, savedChoices, splitModels, type ConfigOption } from "../session";
  import { hub, recentFolders, waitsOnYou } from "../state.svelte";
  import { prefs } from "../prefs.svelte";
  import { engines, lastEngine, modelsFor, projectEngine, projectStart, rememberEngine, startDefaults } from "../providers.svelte";
  import { pickDay } from "../overview.svelte";
  import AwayCard from "./AwayCard.svelte";
  import Board from "./Board.svelte";
  import Scenes from "./Scenes.svelte";
  import CardBody from "./CardBody.svelte";
  import ProjectIcon from "./ProjectIcon.svelte";
  import NewMachineCard from "./NewMachineCard.svelte";
  import SignInCard from "./SignInCard.svelte";
  import Composer from "./Composer.svelte";
  import type { ThreadReach } from "../bindings/ThreadReach";
  import FolderPicker, { chatFolder, scratchFolder } from "./FolderPicker.svelte";
  import { warmer, type Start } from "../warm";
  import HostChip from "./HostChip.svelte";
  import { askCloudSupport, cloudLabel, cloudProviderFor, cloudSupport } from "../cloud.svelte";
  import Icon from "./Icon.svelte";

  // Two pages share the message box: Home (a board of cards about what is going on, with the box
  // at the bottom) and New thread (nothing but the box). One component so what you typed, the
  // folder and the machine stay when you go from one to the other.
  let { composer = $bindable(), mode = "home" }: { composer?: Composer; mode?: "home" | "new" } = $props();

  let editing = $state(false);
  let undoable = $state(false);
  let board = $state<Board>();
  /** The Devlog card's day link: Overview has the day to show it in. */
  function showDay(date: string) {
    pickDay("day", date);
    hub.show({ view: "overview" });
  }

  // Where the thread runs: this machine unless picked, or the one a draft came from.
  let host = $state<string | null>(null);
  const remote = $derived(host && host !== hub.localHost ? host : null);
  // Home history is not a coding destination. Keep it out of implicit project defaults.
  // A scratch thread's own folder is never offered again: "No project" makes a fresh one.
  const folders = $derived(
    (remote
      ? [...new Set(hub.threads.filter((t) => t.host === remote).map((t) => t.cwd))]
      : [...new Set([...recentFolders(), ...hub.threads.filter((t) => !t.host).map((t) => t.cwd)])]
    ).filter(codingFolderDefault),
  );
  let folder = $state(recentFolders().find(codingFolderDefault) ?? "");
  const chat = $derived(hub.world === "chat" && mode === "new");
  $effect(() => {
    // Not in Chat: a chat has no project, so the last project must not fill the folder in again.
    if (!chat && !folder && folders[0]) folder = folders[0];
  });
  // In focus mode a new thread starts in the project in focus, wherever the last one ran.
  $effect(() => {
    const f = prefs.focusOn;
    if (!f || untrack(() => hub.draft)) return;
    untrack(() => {
      host = f.host;
      folder = f.cwd;
    });
  });
  function runOn(name: string) {
    cloud = false;
    const next = name === hub.localHost ? null : name;
    if (next === remote) return;
    const from = remote;
    host = next;
    follow(from, folder.trim());
  }

  /** A folder was picked on another machine in the picker: the thread runs there, in that folder. */
  function pickedOn(name: string | null) {
    host = name;
    elsewhere = null;
    sameRepo = null;
    carried = null;
  }

  // The same repository on the machine picked, as in T3 Code: a repository is known by its remote,
  // so moving to clanker finds its clone there, or offers to clone it.
  // Not there yet: it is brought from the machine it is on, history and .env files with it, so
  // the other machine needs no GitHub login and no secrets typed in again.
  let elsewhere = $state<{ name: string; from: string | null; path: string; target: string; shown: string; host: string | null } | null>(null);
  let sameRepo = $state<{ key: string; from: string | null; path: string } | null>(null);
  let cloning = $state(false);
  /** What the last carry did, in a sentence. */
  let carried = $state<string | null>(null);
  async function follow(from: string | null, path: string) {
    const to = remote;
    elsewhere = null;
    sameRepo = null;
    carried = null;
    folder = "";
    if (!path) {
      folder = folders[0] ?? "";
      return;
    }
    try {
      const src = await api.repoOf(path, from);
      // A project with no remote (made a moment ago) has no clone to find: it is brought over.
      const there = await api.findRepo(src.key ?? "", to);
      if (remote !== to) return;
      if (src.key && there.found[0]) {
        folder = followed = there.found[0];
        sameRepo = { key: src.key, from, path };
      } else {
        const name = (src.key ?? path.replace(/[\\/]+$/, "")).split(/[\\/]/).pop()!;
        const sep = there.root.includes("\\") ? "\\" : "/";
        const target = there.root.replace(/[\\/]+$/, "") + sep + name;
        const shown = target.startsWith(there.home + sep) ? "~" + target.slice(there.home.length) : target;
        elsewhere = { name, from, path, target, shown, host: to };
      }
    } catch {
      if (remote === to && !folder) folder = folders[0] ?? "";
    }
  }
  async function cloneThere() {
    if (!elsewhere || cloning) return;
    cloning = true;
    try {
      const e = elsewhere;
      const b = await api.bringProject({ from: e.from, path: e.path, to: e.host, target: e.target });
      folder = followed = b.folder;
      elsewhere = null;
      carried = said(b, e.from);
    } catch (e) {
      hub.fail(e as Error);
    } finally {
      cloning = false;
    }
  }
  /** The .env files a clone there lacks, from where it came. Files already there stay. */
  async function bringFiles() {
    if (!sameRepo || cloning) return;
    cloning = true;
    try {
      const b = await api.bringProject({ from: sameRepo.from, path: sameRepo.path, to: remote, target: folder });
      carried = b.files.length ? said(b, sameRepo.from) : "It already has every .env file.";
    } catch (e) {
      hub.fail(e as Error);
    } finally {
      cloning = false;
    }
  }
  function said(b: Brought, from: string | null): string {
    const where = from ?? hub.localHost;
    const files = b.files.length ? `${b.files.length === 1 ? b.files[0] : `${b.files.length} env files`} copied` : "no env files to copy";
    const how = b.via === "bundle" ? `History came from ${where}` : b.via === "clone" ? "Cloned from its remote (too big to carry)" : "Already there";
    const left = b.dirty ? `. ${b.dirty} uncommitted ${b.dirty === 1 ? "change stays" : "changes stay"} on ${where}` : "";
    return `${how}, ${files}${left}.`;
  }
  $effect(() => {
    // Typing another folder drops the notes about the last one.
    if (folder && elsewhere) elsewhere = null;
  });
  let followed = "";
  $effect(() => {
    if (sameRepo && folder !== followed) sameRepo = null;
  });

  // A handoff (or any draft) lands here: its folder is picked and its text is ready to edit.
  $effect(() => {
    const draft = hub.draft;
    if (!draft || !composer) return;
    asking = !!draft.everything;
    if (!draft.everything && draft.from !== undefined && draft.from !== draft.host) {
      // Carried over from a thread on another machine: find the same repository there.
      host = draft.host;
      follow(draft.from, draft.cwd);
    } else if (!draft.everything) {
      host = draft.host;
      folder = draft.cwd;
    }
    composer.fill(draft.text);
    for (const a of draft.attachments ?? []) composer.attach(a);
    hub.draft = null;
  });

  const summary = $derived.by(() => {
    const parts: string[] = [];
    const waiting = hub.active.filter(waitsOnYou).length;
    if (waiting) parts.push(`${waiting} waiting`);
    if (hub.running.length) parts.push(`${hub.running.length} running`);
    const ports = hub.host?.listeners.filter((l) => l.origin === "thread").length ?? 0;
    if (ports) parts.push(`${ports} ${ports === 1 ? "port" : "ports"} open`);
    const focus = prefs.focusOn ? `Focused on ${prefs.focusOn.name}. ` : "";
    if (!parts.length) return focus.trim();
    return focus + parts.join(", ") + ".";
  });

  async function system() {
    const picked = await pickFolder(folder);
    if (picked) folder = picked;
  }

  // Model, effort and mode for the next thread. Model and effort carry over from the last thread
  // that reported them, as in Claude Code; the permission mode starts from the engine's default
  // unless picked here.
  // "auto" was Auto, which is gone: it reads as no pick.
  let pickedEngine = $state<string | null>(lastEngine() === "auto" ? null : lastEngine());
  const engine = $derived(pickedEngine && (!engines.loaded || engines.names.includes(pickedEngine)) ? pickedEngine : engines.default);
  let chosen = $state<Record<string, Record<string, SettingValue>>>({});
  // Settings > Engines can pin what every new thread starts on; a pick here then holds for this one.
  // "~/srv/game/" from the picker is the folder ~/srv/game.
  const folderOf = (f: string) => (f.trim().length > 1 ? f.trim().replace(/(?<!^[A-Za-z]:)[\\/]+$/, "") : f.trim());
  // A project's own model and effort (its exception there) lay over that, for the engine it starts on.
  const own = $derived(projectStart(folderOf(folder)));
  const ownOptions = $derived(own?.options && (own.engine ?? engines.default) === engine ? own.options : {});
  const choices = $derived(chosen[engine] ?? { ...savedChoices(engine), ...(startDefaults.by[engine] ?? {}), ...ownOptions });
  const isMode = (o: ConfigOption) => o.category === "mode" || o.id === "mode";
  // What an unpicked mode shows: the mode the core starts a new thread in (engine's auto mode,
  // Hermes's dont_ask; `DEFAULT_MODE` in engine/acp.rs). Nothing is sent, so the core picks it.
  const startMode = (o: ConfigOption) => {
    for (const w of ["auto", "dont_ask"]) {
      const c = o.choices.find((c) => String(c.value).toLowerCase() === w || c.name.toLowerCase() === w);
      if (c) return c.value;
    }
    return o.choices.find((c) => c.value === "default")?.value ?? o.choices[0]?.value;
  };
  const session = $derived.by(() => {
    const options = knownOptions(engine).filter((o) => o.choices.length || o.type === "boolean");
    for (const o of options) {
      const chosen = choices[o.id];
      if (chosen !== undefined && (o.type === "boolean" || o.choices.some((c) => c.value === chosen))) o.value = chosen;
      else if (isMode(o)) o.value = startMode(o) ?? o.value;
    }
    return { ...emptySession(), options };
  });
  const settings = $derived(
    Object.fromEntries(
      session.options.filter((o) => (!isMode(o) || o.id in choices)).map((o) => [o.id, o.value]),
    ),
  );
  function setting(id: string, value: SettingValue) {
    chosen = { ...chosen, [engine]: { ...choices, [id]: value } };
    saveChoices(engine, chosen[engine]);
  }
  // A project with its own engine (Settings > Engines) starts its threads there when its folder
  // is picked; picking another engine here still wins for this message.
  // Only a change of folder moves the engine, and leaving that project hands back what was
  // picked before, unless you picked another engine meanwhile.
  let autoEngine: string | null = null;
  let autoFolder: string | undefined = undefined;
  $effect(() => {
    const at = folderOf(folder);
    const own = projectEngine(at);
    const key = `${at}|${engines.loaded}`;
    if (key === autoFolder) return;
    autoFolder = key;
    untrack(() => {
      if (own && engines.names.includes(own)) {
        if (pickedEngine !== own) autoEngine = own;
        pickedEngine = own;
      } else if (autoEngine && pickedEngine === autoEngine) {
        const last = lastEngine();
        pickedEngine = last === "auto" ? null : last;
        autoEngine = null;
      }
    });
  });
  function pickEngine(name: string, model?: SettingValue) {
    pickedEngine = name;
    rememberEngine(name);
    const option = modelsFor(name);
    if (option && model !== undefined) setting(option.id, model);
  }

  // Several attempts at once: the same task in separate worktrees, by default each on another
  // model, so the best one can be picked.
  let tries = $state(1);
  let optsOpen = $state(false);
  let picked = $state<SettingValue[]>([]);
  const modelOption = $derived(session.options.find((o) => o.category === "model"));
  const modelChoices = $derived.by(() => {
    if (!modelOption) return [];
    const top = splitModels(modelOption).top;
    const current = top.find((c) => c.value === modelOption.value);
    return current ? [current, ...top.filter((c) => c !== current)] : top;
  });
  const tryModels = $derived(
    Array.from({ length: tries }, (_, i) => picked[i] ?? modelChoices[i % Math.max(modelChoices.length, 1)]?.value ?? null),
  );
  function cycle(i: number) {
    const at = modelChoices.findIndex((c) => c.value === tryModels[i]);
    const next = [...tryModels];
    next[i] = modelChoices[(at + 1) % modelChoices.length].value;
    picked = next;
  }
  const choiceName = (v: SettingValue | null) => {
    const c = modelChoices.find((c) => c.value === v);
    return c ? modelLabel(c) : "Default";
  };

  // Asking about everything starts a thread in Hub's desk folder, with all of Hub as its tools (`hub_tools.rs`).
  let asking = $state(false);
  // Run in the cloud instead of on a machine: Claude's or Codex's own servers, by the model picked
  // (`cloud.svelte.ts`). Only for an engine that has a cloud, and only against a core that can.
  let cloud = $state(false);
  const cloudFor = $derived(cloudProviderFor(engine));
  askCloudSupport();
  const cloudOn = $derived(cloud && !!cloudFor && cloudSupport.has && !asking && !chat);
  $effect(() => {
    // The shared composer keeps the draft, but New thread must not inherit Home's scope.
    if (mode === "new") asking = untrack(() => !!hub.draft?.everything);
  });

  // The engine starts while the message is typed, so it is up when it goes (`warm.ts`).
  // In the Chat world a new thread is an everyday chat: no project, a folder of its own under `chats/`
  // (`chat` is declared above `folder`: nothing below may refill the folder or pick it in Chat).
  const noFolder = () => (chat ? chatFolder(remote, api) : scratchFolder(remote, api));
  $effect(() => {
    if (chat) folder = "";
  });
  const warm = warmer(noFolder);
  const startOf = (): Start => ({ engine: tries > 1 ? "auto" : engine, cwd: chat ? "" : folderOf(folder), host: remote, settings });
  function typing() {
    if (!asking && !cloudOn) warm.typing(startOf());
  }

  // Ctrl Enter starts the thread and stays here for the next one.
  // Design first: the new thread plans in a design document before any code (`design.rs`).
  let design = $state(false);

  let reach = $state<ThreadReach>({});
  async function start(text: string, attachments: Attachment[], original?: string, how?: { next?: boolean }) {
    const open = !how?.next;
    // The thread opens now, with the message and "Starting"; the core catches up behind it.
    const shown = open
      ? hub.showStarting({ cwd: asking ? "" : chat ? STARTING_CHAT : folderOf(folder), message: text, attachments, engine: asking ? undefined : engine, host: asking ? null : remote })
      : undefined;
    const fail = (e: unknown) => {
      if (shown) hub.failStandIn(shown, folder, text, attachments, remote);
      throw e;
    };
    if (asking) {
      const desk = await api.deskFolder().catch(fail);
      const t = await hub.newThread(desk.folder, text, attachments, settings, original, undefined, undefined, undefined, open, undefined, shown);
      if (!open) offer(t);
      return;
    }
    const startAs = startOf();
    // The cloud is for this thread only; the next one starts where it always does.
    const inCloud = cloudOn;
    cloud = false;
    const atts = tries > 1 ? tryModels.map((m) => (modelOption && m != null ? { [modelOption.id]: m } : {})) : undefined;
    tries = 1;
    picked = [];
    const warmed = inCloud ? {} : await warm.take(startAs);
    // No project picked: the thread gets a fresh scratch folder on its machine.
    // A chat always gets a folder of its own under chats/, whatever the box held before.
    if (chat || !folder.trim()) folder = warmed.scratch ?? (await noFolder().catch(fail));
    const cwd = folderOf(folder);
    const designFirst = design;
    design = false;
    const reachWith = chat ? undefined : (reach.folders?.length || reach.connections_off?.length ? reach : undefined);
    reach = {};
    const t = await hub.newThread(cwd, text, attachments, settings, original, remote ?? undefined, atts, engine, open, warmed.warm, shown, designFirst, reachWith, inCloud);
    if (!open) offer(t);
  }

  // How much room there is above the heading on New thread. The scene shows only when it fits there.
  let freshEl = $state<HTMLElement>();
  let midEl = $state<HTMLElement>();
  let aboveH = $state(0);
  let room = $state(0);
  $effect(() => {
    if (!freshEl || !midEl) return;
    const measure = () => (room = midEl!.getBoundingClientRect().top - freshEl!.getBoundingClientRect().top + freshEl!.scrollTop - 26);
    measure();
    const ro = new ResizeObserver(measure);
    ro.observe(freshEl);
    ro.observe(midEl);
    return () => ro.disconnect();
  });

  /** Ctrl Enter started a thread in the background: a tab on the box offers to open it for a while. */
  let started = $state<{ id: string; title: string } | null>(null);
  let startedTimer: ReturnType<typeof setTimeout> | undefined;
  function offer(t: { id: string; title: string }) {
    started = { id: t.id, title: t.title };
    clearTimeout(startedTimer);
    startedTimer = setTimeout(() => (started = null), 8000);
  }
</script>

{#snippet tools()}
  {#if editing}
    <button class="ghost" onclick={() => board?.back()} disabled={!undoable} data-tip="Undo the last change (Ctrl Z)">Undo</button>
    <button class="ghost" onclick={() => board?.reset()} data-tip="Back to the cards Home starts with">Reset</button>
  {/if}
  <button class="customize" class:on={editing} onclick={() => (editing = !editing)}>{editing ? "Done" : "Customize"}</button>
{/snippet}

{#snippet custom()}
  <AwayCard />
  <Board bind:this={board} page="home" {editing} bind:undoable empty="No cards on Home. Press Customize and add some.">
    {#snippet content(id)}<CardBody {id} show={showDay} />{/snippet}
  </Board>
{/snippet}

{#if mode === "home"}
  <div class="scroll">
    <div class="column">
      <NewMachineCard />
      <SignInCard />
      <!-- Hub picks the scene (Auto), a tab picks another; Custom is the board of cards. -->
      <Scenes {summary} {custom} {tools} />
    </div>
  </div>
{/if}

<svelte:window onpointerdown={(e) => { if (optsOpen && !(e.target as Element).closest?.(".opts-wrap")) optsOpen = false; }} />

{#snippet box()}
<div class="dock">
  <Composer
    bind:this={composer}
    placeholder={asking ? "What's going on? Or: fix X in Atlas" : chat ? "Ask anything" : "What should we work on?"}
    {engine}
    {session}
    onengine={pickEngine}
    cwd={asking || chat ? undefined : folder.trim() || undefined}
    host={asking ? null : remote}
    onsubmit={start}
    {started}
    onopenstarted={() => started && hub.open(started.id)}
    ontyping={typing}
    onsetting={setting}
    findFiles={folder.trim() && !asking && !chat ? (q) => api.filesIn(folder.trim(), q, remote) : undefined}
    onerror={(e) => hub.fail(e)}
    {design}
    ondesign={asking || chat ? undefined : (on) => (design = on)}
    bind:reach
  >
    <label class="folder">
      {#if asking}
        <Icon name="threads" size={14} />
        <span class="desk">All of Hub · sees every thread and can start new ones</span>
      {:else if chat}
        <Icon name="threads" size={14} />
        <span class="desk">Everyday chat · no project, no files</span>
      {:else}
        <FolderPicker
          bind:value={folder}
          host={remote}
          onhost={(name) => pickedOn(name)}
          system={inTauri && !remote ? system : undefined}
          placeholder={`No project · pick one on ${remote ?? "this machine"}, or start in a scratch folder`}
        />
        <!-- Where it runs and how many attempts: rarely changed, so they wait in a menu. -->
        <span class="opts-wrap">
          <button
            class="browse opts"
            class:on={optsOpen}
            class:set={!!remote || tries > 1 || cloudOn}
            data-tip="Where the thread runs, and attempts"
            onclick={(e) => { e.preventDefault(); optsOpen = !optsOpen; }}
          >
            {#if cloudOn}<Icon name="cloudrun" size={14} />{:else if hub.hosts.length > 1}<HostChip name={remote ?? hub.localHost} />{/if}
            {#if tries > 1}<span>{hub.hosts.length > 1 ? "· " : ""}{tries} attempts</span>{/if}
            {#if hub.hosts.length <= 1 && tries === 1 && !cloudOn}<Icon name="more" size={14} />{:else}<Icon name="down" size={12} />{/if}
          </button>
          {#if optsOpen}
            <div class="opts-menu" transition:pop|global role="menu">
              {#if hub.hosts.length > 1 || cloudFor}
                <div class="menu-title">Run on</div>
                {#each hub.hosts as h (h.name)}
                  <button
                    class="opt"
                    class:on={!cloudOn && (remote ?? hub.localHost) === h.name}
                    disabled={h.state !== "online"}
                    data-tip={h.state === "online" ? `Start the thread on ${h.name}` : `${h.name} is ${h.state}${h.error ? `: ${h.error}` : ""}`}
                    onclick={(e) => { e.preventDefault(); optsOpen = false; runOn(h.name); }}
                  >
                    <HostChip name={h.name} /><span class="grow"></span>
                    {#if h.state !== "online"}<span class="muted">{h.state}</span>{:else if !cloudOn && (remote ?? hub.localHost) === h.name}<Icon name="check" size={13} />{/if}
                  </button>
                {/each}
                {#if cloudFor}
                  <!-- One entry: the model picked beside decides whose cloud it is. -->
                  <button
                    class="opt"
                    class:on={cloudOn}
                    disabled={!cloudSupport.has}
                    data-tip={cloudSupport.has ? `Run it on ${cloudFor === "claude" ? "Anthropic" : "OpenAI"}'s servers with your ${cloudFor === "claude" ? "Claude" : "ChatGPT"} subscription. Needs the folder's repository on GitHub, with the branch pushed.` : "This machine's Hub needs the next version to start threads in the cloud"}
                    onclick={(e) => { e.preventDefault(); optsOpen = false; cloud = true; }}
                  >
                    <Icon name="cloudrun" size={14} />Cloud<span class="grow"></span>
                    {#if !cloudSupport.has}<span class="muted">next version</span>{:else if cloudOn}<Icon name="check" size={13} />{:else}<span class="muted">{cloudLabel(cloudFor)}</span>{/if}
                  </button>
                {/if}
                <div class="divider"></div>
              {/if}
              <div class="menu-title" data-tip="The same task in separate git worktrees, each its own thread; pick the best one afterwards">Attempts · same task in parallel worktrees</div>
              <div class="seg">
                {#each [1, 2, 3] as n (n)}
                  <button class:on={tries === n} onclick={(e) => { e.preventDefault(); tries = n; }}>{n}</button>
                {/each}
              </div>
              {#if tries > 1 && modelChoices.length}
                {#each tryModels as m, i (i)}
                  <button class="opt" data-tip="Click for another model" onclick={(e) => { e.preventDefault(); cycle(i); }}>
                    <span class="muted">{i + 1}</span>{choiceName(m)}<span class="grow"></span><Icon name="chevron" size={12} />
                  </button>
                {/each}
              {/if}
            </div>
          {/if}
        </span>
      {/if}
      {#if !chat}
        <button class="browse" class:on={asking} data-tip="Ask about all of Hub instead of working in one folder" onclick={() => (asking = !asking)}>
          {asking ? "Back to a folder" : "Ask about everything"}
        </button>
      {/if}
    </label>
    {#if !asking && elsewhere}
      <div class="carry">
        <Icon name="branch" size={13} />
        <span><b>{elsewhere.name}</b> is not on {elsewhere.host ?? hub.localHost} yet.</span>
        <button class="browse" disabled={cloning} data-tip={`Copies its history and .env files from ${elsewhere.from ?? hub.localHost}; no GitHub login needed there`} onclick={cloneThere}>
          {cloning ? "Bringing…" : `Bring it to ${elsewhere.shown}`}
        </button>
      </div>
    {:else if !asking && carried && folder}
      <div class="carry quiet"><Icon name="check" size={12} /> {carried}</div>
    {:else if !asking && sameRepo && folder}
      <div class="carry quiet">
        <Icon name="check" size={12} /> Same repository on {remote ?? hub.localHost}
        <button class="browse" disabled={cloning} data-tip={`Copies the .env files it lacks from ${sameRepo.from ?? hub.localHost}; files already there stay`} onclick={bringFiles}>
          {cloning ? "Copying…" : "Bring .env files"}
        </button>
      </div>
    {/if}
  </Composer>
</div>
{/snippet}

{#if mode === "home"}
  {@render box()}
{:else}
  <div class="fresh" bind:this={freshEl}>
    <div class="mid" bind:this={midEl}>
      <!-- The scene sits above the heading and takes no room: the box stays where it is. -->
      {#if !chat}<div class="above" class:hide={aboveH > room} bind:clientHeight={aboveH}><Scenes compact {summary} /></div>{/if}
    <div class="intro">
      {#if chat}
        <h1>Ask anything</h1>
        <p class="sub">A new chat{hub.hosts.length > 1 ? ` on ${remote ?? hub.localHost}` : ""}.</p>
      {:else}
        <h1>What should we work on?</h1>
        <p class="sub">New thread{asking ? " about all of Hub" : folder.trim() ? ` in ${folderName(folderOf(folder))}` : " in a scratch folder"}{hub.hosts.length > 1 ? ` on ${remote ?? hub.localHost}` : ""}.</p>
      {/if}
    </div>
    {@render box()}
    {#if !asking && !chat && folders.length}
      <div class="recent">
        <div class="label">Recent projects</div>
        <div class="chips">
          {#each folders.slice(0, 6) as f (f)}
            <button class="chip" class:on={folderOf(folder) === f} onclick={() => (folder = f)} data-tip={f}>
              <ProjectIcon path={f} host={remote} name={folderName(f)} size={14} />{folderName(f)}
            </button>
          {/each}
          <button class="chip" class:on={!folder.trim()} onclick={() => (folder = "")} data-tip="Start in a fresh scratch folder">No project</button>
        </div>
      </div>
    {/if}
    </div>
  </div>
{/if}

<style>
  .scroll { flex: 1; overflow-y: auto; min-height: 0; }
  .customize, .ghost { font-size: 13px; color: var(--text-2); background: var(--surface-hover); border-radius: var(--r-md); padding: 3px 10px; flex: none; }
  .ghost { background: none; color: var(--muted); }
  .customize:hover, .ghost:hover { color: var(--text); }
  .customize.on { background: var(--accent); color: var(--bg); }
  .ghost:disabled { opacity: 0.4; cursor: default; }
  /* New thread: the box in the middle of the page, with where it could start under it. */
  .fresh { flex: 1; min-height: 0; overflow-y: auto; display: flex; flex-direction: column; justify-content: center; padding-bottom: 6vh; }
  .fresh .mid { position: relative; width: 100%; }
  /* Above the heading, out of the flow, so adding it moves nothing. Too short a window has no room for it. */
  .fresh .above { position: absolute; left: 0; right: 0; bottom: 100%; max-width: var(--chat); width: 100%; margin: 0 auto 26px; padding: 0 28px; }
  .fresh .above.hide { visibility: hidden; }
  .fresh .intro { max-width: var(--chat); width: 100%; margin: 0 auto; padding: 0 28px; text-align: center; }
  .fresh h1 { margin: 0 0 6px; font-size: calc(var(--title) + 4px); }
  .fresh .sub { margin: 0 0 14px; color: var(--muted); font-size: 14px; }
  .recent { max-width: var(--chat); width: 100%; margin: 0 auto; padding: 0 28px; }
  .recent .label { margin: 4px 0 8px; }
  .chips { display: flex; gap: 8px; flex-wrap: wrap; }
  .chip { display: inline-flex; align-items: center; gap: 8px; height: 30px; padding: 0 12px 0 8px; border-radius: var(--r-md); background: var(--surface); border: 1px solid var(--line); color: var(--text-2); font-size: 13px; max-width: 220px; white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
  .chip:hover { color: var(--text); background: var(--surface-hover); }
  .chip.on { border-color: color-mix(in srgb, var(--accent) 50%, var(--line)); color: var(--text); }
  .column { padding: var(--page-top) var(--gutter) 24px; }
  h1 { color: var(--text); font-size: var(--title); font-weight: 500; letter-spacing: -0.01em; margin: 14px 0 6px; }
  .dock { max-width: var(--chat); width: 100%; margin: 0 auto; padding: 8px 28px 16px; }
  .folder { display: flex; align-items: center; gap: 8px; color: var(--muted); border-bottom: 1px solid var(--line); margin: -2px 0 8px; padding-bottom: 8px; }
  .browse { color: var(--text-3); font-size: 12px; padding: 2px 8px; border-radius: var(--r-sm); background: var(--surface-hover); }
  .browse:hover { color: var(--text); }
  .browse.on { color: var(--accent); background: var(--accent-bg); }
  .opts-wrap { position: relative; display: inline-flex; }
  .opts { display: inline-flex; align-items: center; gap: 6px; }
  .opts.set { color: var(--text-2); }
  .opts :global(.chip) { color: inherit; }
  .opts-menu {
    position: absolute; z-index: 20; right: 0; bottom: calc(100% + 6px); min-width: 230px;
    background: var(--surface-2); border: 1px solid var(--line-strong); border-radius: var(--r-lg);
    padding: 4px; box-shadow: 0 12px 32px rgba(0, 0, 0, 0.45);
  }
  .menu-title { color: var(--muted); font-size: 12px; padding: 4px 8px 3px; white-space: nowrap; }
  .opt { display: flex; align-items: center; gap: 8px; width: 100%; padding: 4px 8px; border-radius: var(--r-md); font-size: 13px; color: var(--text-2); font-family: var(--font, inherit); }
  .opt:hover:not(:disabled) { background: var(--surface-hover); color: var(--text); }
  .opt.on { color: var(--text); }
  .opt:disabled { opacity: 0.5; cursor: default; }
  .opt :global(.chip) { color: inherit; }
  .opt :global(svg:last-child) { color: var(--accent); }
  .grow { flex: 1; }
  .muted { color: var(--muted); font-size: 12px; }
  .divider { height: 1px; background: var(--line-strong); margin: 4px 6px; }
  .seg { display: flex; gap: 2px; padding: 2px 8px 6px; }
  .seg button { padding: 1px 10px; border-radius: var(--r-sm); line-height: 20px; color: var(--muted); font-size: 13px; }
  .seg button:hover { color: var(--text-2); background: var(--surface-hover); }
  .seg button.on { color: var(--text); background: var(--surface-hover); }
  .carry { display: flex; align-items: center; gap: 8px; color: var(--text-3); font-size: 13px; margin: -4px 0 8px; }
  .carry .browse { white-space: nowrap; overflow: hidden; text-overflow: ellipsis; min-width: 0; font-family: var(--mono); }
  .carry > span { white-space: nowrap; }
  .carry b { font-weight: 500; color: var(--text-2); }
  .carry.quiet { color: var(--muted); font-size: 12px; }
  .carry :global(svg) { color: var(--muted); flex: none; }
  .desk { flex: 1; font-size: 13px; color: var(--text-3); }
</style>
