<!-- A project is the folder threads run in. This page is the overview before diving in: the work (plan, docs, threads) on the left, the repository, GitHub and stack on the right, and the docs, files and services each on a tab of their own. `host` is the machine the folder is on, or null for this one; requests go to its core.
     Used by App.svelte. -->
<script lang="ts">
  import { quiet, quietly } from "../quiet";
  import { poll } from "../poll";
  import Quiet from "./Quiet.svelte";
  import ProjectRunning from "./ProjectRunning.svelte";
  import Tabs from "./Tabs.svelte";
  import Ref from "./Ref.svelte";
  import ProjectIcon from "./ProjectIcon.svelte";
  import { machineGlyph, repoName, shortPath } from "../refs";
  import { api } from "../api";
  import { scratchFolder } from "./FolderPicker.svelte";
  import { warmer, type Start } from "../warm";
  import type { Attachment } from "../bindings/Attachment";
  import type { FileText } from "../bindings/FileText";
  import type { GithubInfo } from "../bindings/GithubInfo";
  import type { ProjectRuntime } from "../bindings/ProjectRuntime";
  import type { ProjectServices } from "../bindings/ProjectServices";
  import type { ServicesDone } from "../bindings/ServicesDone";
  import type { PlanItem } from "../bindings/PlanItem";
  import type { ProjectInfo } from "../bindings/ProjectInfo";
  import type { ProjectTrees } from "../bindings/ProjectTrees";
  import { hubdHas } from "../compat";
  import Switch from "./Switch.svelte";
  import type { SettingValue } from "../bindings/SettingValue";
  import type { ProjectHealth } from "../bindings/ProjectHealth";
  import type { ProjectDay } from "../bindings/ProjectDay";
  import type { Stats } from "../bindings/Stats";
  import { ago, byProject, folderName, isScratch, since, statusLabel, clip, threadProject } from "../format";
  import { openUrl } from "../native";
  import { emptySession, knownOptions, saveChoices, savedChoices, type ConfigOption } from "../session";
  import { engines, lastEngine, modelsFor, rememberEngine } from "../providers.svelte";
  import { menu } from "../menu.svelte";
  import { motion } from "../motion";
  import { fileMenu, join, linkMenu, projectMenu, serverMenu, threadMenu } from "../menus.svelte";
  import { hub, unread } from "../state.svelte";
  import { projectCounts } from "../threadFlags";
  import FilePreview from "./FilePreview.svelte";
  import { fileKind, isBinary } from "../filePreview";
  import Composer from "./Composer.svelte";
  import DocsView from "./DocsView.svelte";
  import ProjectLibrary from "./ProjectLibrary.svelte";
  import FileTree from "./FileTree.svelte";
  import FocusCard from "./FocusCard.svelte";
  import TestTab from "./TestTab.svelte";
  import RulesTab from "./RulesTab.svelte";
  import GithubTab from "./GithubTab.svelte";
  import HostChip from "./HostChip.svelte";
  import Icon from "./Icon.svelte";
  import OwnerMark from "./OwnerMark.svelte";
  import ProjectApis from "./ProjectApis.svelte";
  import ProjectRecipes from "./ProjectRecipes.svelte";

  // A project is the folder threads run in. This page is the overview before diving in: the work
  // (plan, docs, threads) on the left, the repository, GitHub and stack on the right, and the
  // docs, files and services each on a tab of their own.
  // `host` is the machine the folder is on, or null for this one; requests go to its core.
  let { path, host = null }: { path: string; host?: string | null } = $props();

  let info = $state<ProjectInfo | null>(null);
  const machine = $derived(hub.hostInfo(host ?? hub.localHost));
  let github = $state<GithubInfo | null>(null);
  let loadError = $state<string | null>(null);

  const here = (t: { cwd: string; host?: string }) => threadProject(t) === path && (t.host ?? null) === host;
  const threads = $derived(hub.active.filter(here));
  const counts = $derived(projectCounts(threads));
  const started = $derived(hub.threads.filter(here).reduce((min, t) => Math.min(min, t.created_at), Infinity));

  async function load() {
    try {
      info = await api.project(path, host);
      loadError = null;
    } catch (e) {
      loadError = e instanceof Error ? e.message : String(e);
    }
  }
  // Agents update the docs and the plan as they work, so reload when a thread changes state.
  $effect(() => {
    void threads.map((t) => t.status).join();
    load();
  });
  $effect(() => {
    const timer = poll(load, 20_000);
    return () => timer();
  });
  $effect(() => {
    const loadGithub = () => quiet(api.projectGithub(path, host).then((g) => (github = g)), "project github");
    loadGithub();
    const timer = poll(loadGithub, 60_000);
    return () => timer();
  });

  // This project's line of the weekly health check and its month of tokens, from Overview's data.
  let health = $state<ProjectHealth | null>(null);
  let stats = $state<Stats | null>(null);
  // Its devlog: the days it moved, read from git and its threads.
  let devlog = $state<ProjectDay[]>([]);
  let devlogOpen = $state<string | null>(null);
  $effect(() => {
    const at = path;
    health = null;
    stats = null;
    devlog = [];
    if (host) return;
    quiet(api.projectDevlog(at, 14, -new Date().getTimezoneOffset()).then((d) => at === path && (devlog = d)), "project devlog");
    const same = (p: string) => p.replace(/\\/g, "/").toLowerCase() === at.replace(/\\/g, "/").toLowerCase();
    quiet(api.checkup().then((r) => at === path && (health = r.projects.find((p) => same(p.path)) ?? null)), "checkup");
    quiet(api.stats(30, -new Date().getTimezoneOffset(), at).then((s) => at === path && (stats = s)), "stats");
  });
  const logDate = (date: string) => new Date(date + "T12:00").toLocaleDateString([], { weekday: "short", day: "numeric", month: "short" });
  const issues = $derived(health?.checks.filter((c) => c.health === "warn" || c.health === "bad") ?? []);
  const money = (n: number) => (n >= 100 ? `$${Math.round(n)}` : `$${n.toFixed(n >= 1 ? 1 : 2)}`);
  const millions = (n: number) => (n >= 1e6 ? `${(n / 1e6).toFixed(1)}M` : n >= 1e3 ? `${Math.round(n / 1e3)}k` : `${n}`);

  // The plan: the last few steps done, greyed out, then what comes next.
  let showWholePlan = $state(false);
  const plan = $derived(info?.plan ?? []);
  const done = $derived(plan.filter((p) => p.done));
  const next = $derived(plan.filter((p) => !p.done));
  const shortPlan = $derived([...done.slice(-3), ...next.slice(0, 3)]);
  const shownPlan = $derived(showWholePlan ? plan : shortPlan);

  async function tick(item: PlanItem) {
    try {
      const items = await api.editPlan({ path, line: item.line, done: !item.done }, host);
      if (info) info.plan = items;
    } catch (e) {
      hub.fail(e);
      load();
    }
  }

  let newStep = $state("");
  async function addStep(e: SubmitEvent) {
    e.preventDefault();
    const text = newStep.trim();
    if (!text) return;
    try {
      const items = await api.editPlan({ path, text }, host);
      newStep = "";
      if (info) {
        info.plan = items;
        info.has_plan = true;
      }
    } catch (err) {
      hub.fail(err);
    }
  }

  // Whether each new thread here gets a worktree of its own. A hubd from before the call has
  // none of it, and a project on another machine is that machine's to say.
  let trees = $state<ProjectTrees | null>(null);
  $effect(() => {
    const at = path;
    trees = null;
    if (host) return;
    hubdHas("project.worktrees") // compat-until: 2026-12-02
      .then((has) => (has ? api.projectTrees(at) : null))
      .then((t) => {
        if (at === path) trees = t;
      })
      .catch(quietly("project worktrees"));
  });
  async function setTrees(on: boolean) {
    try {
      trees = await api.setProjectTrees(path, on);
    } catch (e) {
      hub.fail(e);
    }
  }

  async function setUpDocs() {
    try {
      await api.setUpDocs(path, host);
      await load();
    } catch (e) {
      hub.fail(e);
    }
  }


  // The file being read on the files tab. Docs open on their own tab, which keeps its place.
  let file = $state<FileText | null>(null);
  function openDoc(rel: string) {
    hub.projectDoc = rel;
    hub.projectTab = "docs";
  }
  /** Kinds the reader fetches itself (bytes, or the core's reading of a spreadsheet or database), not as text. */
  const readsItself = (rel: string) => isBinary(fileKind(rel)) || fileKind(rel) === "sheet" || fileKind(rel) === "database";
  async function openFile(rel: string) {
    try {
      // Pictures, PDFs, sound and video are read by the reader itself, as bytes.
      file = readsItself(rel) ?{ rel, text: "", truncated: false } : await api.projectFile(path, rel, host);
    } catch (e) {
      hub.fail(e);
    }
  }

  async function openFolder(how: "editor" | "terminal" | "files") {
    try {
      await api.openFolder(path, how);
    } catch (e) {
      hub.fail(e);
    }
  }

  // What the project has running and what it uses, worked out by the core from the processes'
  // environment, their folders, their connections and docker compose. Nothing is tagged by hand.
  let runtime = $state<ProjectRuntime | null>(null);
  const loadRuntime = () => quiet(api.projectRuntime(path, host).then((r) => (runtime = r)), "project runtime");
  $effect(() => {
    loadRuntime();
    const timer = poll(loadRuntime, 5000);
    return () => timer();
  });
  const runningCount = $derived((runtime?.servers.length ?? 0) + (runtime?.containers.length ?? 0));
  const threadTitle = (id: string | null) => (id ? hub.threads.find((t) => t.id === id)?.title ?? null : null);
  /** The Hub project a used server runs in, if any. */
  const projectOf = (cwd: string | null) => {
    if (!cwd) return null;
    const norm = (p: string) => p.replace(/[\\/]+$/, "").toLowerCase();
    const c = norm(cwd);
    return byProject(hub.active, hub.projects).find((p) => p.host === host && p.cwd !== path && (c === norm(p.cwd) || c.startsWith(norm(p.cwd) + (p.cwd.includes("\\") ? "\\" : "/")))) ?? null;
  };
  // What the project starts (its Procfile.dev, or a guess from its manifests) and Start all /
  // Stop all, which also bring compose up or down and, for Stop all, the threads' engines.
  let services = $state<ProjectServices | null>(null);
  const loadServices = () => quiet(api.services(path, host).then((s) => (services = s)), "services");
  $effect(() => {
    loadServices();
    const timer = poll(loadServices, 5000);
    return () => timer();
  });
  const servicesUp = $derived(services?.services.filter((s) => s.pids.length).length ?? 0);
  let acting = $state<string | null>(null);
  let doneNote = $state<string | null>(null);
  const list = (names: string[]) => (names.length < 3 ? names.join(" and ") : `${names.slice(0, -1).join(", ")} and ${names.at(-1)}`);
  const plural = (n: number, one: string, many: string) => `${n} ${n === 1 ? one : many}`;
  function describe(done: ServicesDone, verb: "Started" | "Stopped") {
    const parts = [...done.services];
    if (done.compose) parts.push("compose");
    if (done.engines) parts.push(plural(done.engines, "engine", "engines"));
    if (done.processes) parts.push(plural(done.processes, "process", "processes"));
    let text = parts.length ? `${verb} ${list(parts)}.` : verb === "Started" ? "Everything was running already." : "Nothing was running.";
    if (done.in_terminal) text += ` ${plural(done.in_terminal, "thread", "threads")} in a terminal left alone.`;
    return text;
  }
  async function runServices(verb: "start" | "stop", name?: string) {
    acting = name ? `${verb}:${name}` : verb;
    try {
      const done = verb === "start" ? await api.startServices(path, name, host) : await api.stopServices(path, name, host);
      if (!name) {
        doneNote = describe(done, verb === "start" ? "Started" : "Stopped");
        setTimeout(() => (doneNote = null), 6000);
      }
      if (done.errors.length) hub.fail(new Error(done.errors.join("; ")));
      await Promise.all([loadServices(), loadRuntime()]);
    } catch (e) {
      hub.fail(e);
    } finally {
      acting = null;
    }
  }
  async function saveGuess() {
    try {
      await api.saveServices(path, host);
      await loadServices();
    } catch (e) {
      hub.fail(e);
    }
  }
  // One service's output, followed while it is open.
  let logOf = $state<string | null>(null);
  let logText = $state("");
  $effect(() => {
    const name = logOf;
    if (!name) return;
    const load = () => quiet(api.serviceLog(path, name, host).then((l) => (logText = l.text)), "service log");
    load();
    const timer = poll(load, 2000);
    return () => timer();
  });

  let stopping = $state<number | null>(null);
  async function stop(pid: number) {
    stopping = pid;
    try {
      await api.stopProcess(pid, host);
      await loadRuntime();
    } catch (e) {
      hub.fail(e);
    } finally {
      stopping = null;
    }
  }

  let composer = $state<ReturnType<typeof Composer> | null>(null);
  // The composer starts a thread in this folder, with the same settings memory as Home.
  // "auto" was Auto, which is gone: it reads as no pick.
  let pickedEngine = $state<string | null>(lastEngine() === "auto" ? null : lastEngine());
  const engine = $derived(pickedEngine && (!engines.loaded || engines.names.includes(pickedEngine)) ? pickedEngine : engines.default);
  let chosen = $state<Record<string, Record<string, SettingValue>>>({});
  const choices = $derived(chosen[engine] ?? savedChoices(engine));
  const isMode = (o: ConfigOption) => o.category === "mode" || o.id === "mode";
  const session = $derived.by(() => {
    const options = knownOptions(engine).filter((o) => o.choices.length || o.type === "boolean");
    for (const o of options) {
      const chosen = choices[o.id];
      if (chosen !== undefined && (o.type === "boolean" || o.choices.some((c) => c.value === chosen))) o.value = chosen;
      else if (isMode(o)) o.value = o.choices.find((c) => c.value === "default")?.value ?? o.choices[0]?.value ?? o.value;
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
  function pickEngine(name: string, model?: SettingValue) {
    pickedEngine = name;
    rememberEngine(name);
    const option = modelsFor(name);
    if (option && model !== undefined) setting(option.id, model);
  }
  // The engine starts while the message is typed, so it is up when it goes (`warm.ts`).
  const warm = warmer(() => scratchFolder(host, api));
  const startOf = (): Start => ({ engine, cwd: isScratch(path) ? "" : path, host: host && host !== hub.localHost ? host : null, settings });
  async function start(text: string, attachments: Attachment[], original?: string) {
    // The thread opens now, with the message and "Starting"; the core catches up behind it.
    const shown = hub.showStarting({ cwd: isScratch(path) ? "" : path, message: text, attachments, engine, host });
    try {
      if (hub.firstRun?.path === path) hub.firstRun = null;
      const warmed = await warm.take(startOf());
      // The Scratch project: each thread still gets its own fresh folder, as "No project" does.
      const cwd = isScratch(path) ? (warmed.scratch ?? (await scratchFolder(host, api))) : path;
      await hub.newThread(cwd, text, attachments, settings, original, host ?? undefined, undefined, engine, true, warmed.warm, shown);
    } catch (e) {
      // newThread has taken the stand-in away already when it was the core that said no.
      hub.failStandIn(shown, path, text, attachments, host);
      throw e;
    }
  }
  /** A thread in this project from the GitHub tab: a pull request to fix, an issue to work on. */
  async function startWith(text: string) {
    await hub.newThread(path, text, [], settings, undefined, host ?? undefined, undefined, engine).catch((e) => hub.fail(e));
  }
  function openPull(n: number) {
    hub.github = { mode: "pulls", pull: n, issue: null };
    hub.projectTab = "github";
  }
  async function startStep(item: PlanItem) {
    await hub
      .newThread(path, `${item.text}\n\nThis is the next step in docs/plan.md. Tick it off there when it is done.`, [], settings, undefined, host ?? undefined, undefined, engine)
      .catch((e) => hub.fail(e));
  }

  const ciState = (g: GithubInfo) => {
    const ci = g.ci;
    if (!ci) return null;
    if (ci.status !== "completed") return { cls: "running", label: "Running" };
    if (ci.conclusion === "success") return { cls: "ok", label: "Passing" };
    if (ci.conclusion === "failure" || ci.conclusion === "timed_out") return { cls: "bad", label: "Failing" };
    return { cls: "idle", label: ci.conclusion || ci.status };
  };
</script>

{#snippet reader(f: FileText)}
  <div class="reader-head"><span class="mono rel">{f.rel}</span></div>
  {#key f.rel}<FilePreview root={path} rel={f.rel} {host} text={readsItself(f.rel) ? null : f.text} ondoc={(doc) => openDoc(doc)} />{/key}
  {#if f.truncated}<p class="none">Only the start of the file is shown.</p>{/if}
{/snippet}

<div class="page">
  <!-- The title, its buttons and the tabs stay put; only what is under them scrolls. -->
  <div class="top">
    <header class="hero" use:menu={() => projectMenu(path, host)}>
      <div class="title-row">
        <span class="avatar"><ProjectIcon {path} {host} name={info?.name ?? folderName(path)} size={20} editable /></span>
        <div class="titles">
          <div class="name-row">
            <h1>{info?.name ?? folderName(path)}</h1>
            {#if health}
              <button class="health" data-tip={issues.map((c) => c.detail).join("\n") || "All good in the last health check"} onclick={() => (hub.view = "overview")}>
                <span class="state {health.health}"></span>{health.health === "bad" ? "Broken" : issues.length ? `${issues.length} to look at` : "Healthy"}
              </button>
            {/if}
          </div>
          <div class="sub">
            {#if repoName(info?.github)}
              <Ref kind="repo" mono label={repoName(info?.github)!} title="Open the GitHub tab" onopen={() => (hub.projectTab = "github")} /><span class="sep">·</span>
            {/if}
            {#if info?.branch}
              <span class="branch"><Ref kind="branch" mono label={info.branch} /></span>
              {#if info.changed}<span class="changed">{info.changed} changed</span>{/if}
              <span class="sep">·</span>
            {/if}
            {#if machine}<Ref kind={machineGlyph(machine.name, machine.local)} label={machine.name} online={machine.state === "online"} title={machine.local ? "This machine" : machine.address} onopen={() => (hub.view = "machines")} /><span class="sep">·</span>{/if}
            <Ref kind="folder" mono label={shortPath(path)} title={path} />
          </div>
        </div>
        <div class="actions">
          {#if !host}
            <button class="btn" onclick={() => openFolder("editor")} data-tip="Open the folder in VS Code"><Icon name="file" size={13} /> Editor</button>
            <button class="btn" onclick={() => openFolder("terminal")} data-tip="Open a terminal in the folder"><Icon name="terminal" size={13} /> Terminal</button>
            <button class="btn" onclick={() => openFolder("files")} data-tip="Show the folder"><Icon name="folder" size={13} /> Folder</button>
          {/if}
          {#if services?.services.length || services?.compose}
            <button class="btn" disabled={!!acting} onclick={() => runServices("start")} data-tip="Start what the project runs: {[...(services?.services.map((s) => s.name) ?? []), ...(services?.compose ? ["docker compose"] : [])].join(", ")}"><Icon name="play" size={13} /> Start all</button>
          {/if}
          <button class="btn danger" disabled={!!acting} onclick={() => runServices("stop")} data-tip="Stop services and engines."><Icon name="stop" size={13} /> Stop all</button>
          {#if info?.github}
            <button class="btn" onclick={() => openUrl(info!.github!)} data-tip={info.github}><Icon name="branch" size={13} /> GitHub</button>
          {/if}
          {#if !host && hub.projects.some((p) => p.path === path)}
            <button class="btn ghost-btn" data-tip="Take it off the project list. The folder and its threads stay." onclick={() => hub.removeProject(path)}>Remove</button>
          {/if}
        </div>
      </div>
      {#if doneNote}<p class="m done-note">{doneNote}</p>{/if}
      {#if hub.firstRun && hub.firstRun.path === path}
        {@const first = hub.firstRun}
        <div class="first" role="note">
          <div class="lines">
            {#if first.thread}
              <p>A setup thread is writing the plan and docs. <button class="link" onclick={() => hub.open(first.thread!)}>Open it</button>.</p>
            {/if}
          </div>
          <button class="ghost" data-tip="Hide" aria-label="Hide" onclick={() => (hub.firstRun = null)}><Icon name="x" size={14} /></button>
        </div>
      {/if}
      {#if info?.summary}<p class="summary">{info.summary}</p>{/if}
      {#if loadError}<p class="error">{loadError}</p>{/if}
    </header>

    <Tabs style="margin: 0" value={hub.projectTab} onchange={(id) => (hub.projectTab = id)} items={[
      { id: "overview", label: "Overview" },
      { id: "library", label: "Library" },
      ...(info?.github ? [{ id: "github" as const, label: "GitHub", count: github?.prs.length || null }] : []),
      { id: "tests", label: "Test", tip: "What is left to try by hand, script by script" },
      { id: "rules", label: "Rules", tip: "What the rules files tell the agents, and when each last changed" },
      { id: "docs", label: "Docs", count: info?.doc_count || null },
      { id: "files", label: "Files" },
      { id: "services", label: "Services", count: runningCount || servicesUp ? Math.max(runningCount, servicesUp) : null },
    ]} />
  </div>
  {#if hub.projectTab === "library"}
    <div class="fill lib">{#key `${host}:${path}`}<ProjectLibrary {path} {host} />{/key}</div>
  {:else if info && hub.projectTab === "docs"}
    <!-- The docs tab fills the height and scrolls its columns itself. -->
    <div class="fill">{#key `${host}:${path}`}<DocsView {path} {host} version={info} doc={hub.projectDoc} ongo={(t) => { hub.projectTab = "docs"; hub.projectDoc = t; }} />{/key}</div>
  {:else}
  <div class="scroll">
    <div class="wrap">
      {#if !info}
        {#if !loadError}<Quiet kind="loading" />{/if}
      {:else if hub.projectTab === "overview"}
        <FocusCard {path} {host} focus={info.focus} onfocus={(f) => info && (info.focus = f)} onstart={startWith} onfill={(t) => composer?.fill(t, true)} />
        <div class="grid">
          <div class="col">
            {#if info.has_plan || !info.focus.exists}
            <section>
              <div class="card-head">
                <h2>Plan</h2>
                {#if plan.length}<span class="n">{done.length} of {plan.length} done</span>{/if}
                {#if plan.length > shortPlan.length}
                  <button class="more" onclick={() => (showWholePlan = !showWholePlan)}>{showWholePlan ? "Show less" : `Show all ${plan.length}`}</button>
                {/if}
              </div>
              {#if plan.length}
                <div class="progress"><span style:width="{(done.length / plan.length) * 100}%"></span></div>
              {/if}
              {#each shownPlan as item (item.line)}
                <div class="step" class:done={item.done}>
                  <button class="box" class:checked={item.done} onclick={() => tick(item)} data-tip={item.done ? "Mark as not done" : "Mark as done"}>
                    {#if item.done}<Icon name="check" size={12} />{/if}
                  </button>
                  <span class="text">{item.text}</span>
                  {#if !item.done}
                    <button class="btn start" onclick={() => startStep(item)} data-tip="Start a thread for this step">Start</button>
                  {/if}
                </div>
              {:else}
                <p class="none">
                  {info.has_plan ? "The plan is empty." : "No plan yet."}
                </p>
              {/each}
              <form class="add" onsubmit={addStep}>
                <Icon name="plus" size={13} />
                <input bind:value={newStep} placeholder="Add a step" spellcheck="false" />
              </form>
              {#if !info.docs_rule}
                <div class="setup">
                  <span>Let the agents keep the plan and the docs up to date as they work.</span>
                  <button class="btn primary" onclick={setUpDocs}>Turn on living docs</button>
                </div>
              {/if}
            </section>
            {/if}

            <section>
              <div class="card-head">
                <h2>Docs</h2><span class="n">{info.docs.length}</span>
                {#if info.docs.length}<button class="more" onclick={() => (hub.projectTab = "docs")}>Open docs</button>{/if}
              </div>
              {#if info.docs.length}
                <div class="docs">
                  {#each info.docs.slice(0, 4) as d (d.rel)}
                    <button class="doc-card" onclick={() => openDoc(d.rel)} use:menu={() => fileMenu(join(path, d.rel), { rel: d.rel, open: () => openDoc(d.rel) })}>
                      <span class="t">{d.title}</span>
                      {#if d.preview}<span class="p">{d.preview}</span>{/if}
                      <span class="m"><span class="mono">{d.rel}</span> · {since(d.modified)}</span>
                    </button>
                  {/each}
                </div>
              {:else}
                <p class="none">No docs.</p>
              {/if}
            </section>

            <section use:motion>
              <div class="card-head">
                <h2>Threads</h2><span class="n">{threads.length}</span>
                {#if counts.running}<span class="tag"><span class="dot running"></span>{counts.running} running</span>{/if}
                {#if counts.asks}<span class="tag"><span class="dot waiting_on_you"></span>{counts.asks} waiting on you</span>{/if}
                {#if counts.failed}<span class="tag"><span class="dot error"></span>{counts.failed} failed</span>{/if}
              </div>
              {#if trees?.root}
                <div class="trees">
                  <div class="what">
                    <span class="name">Own worktree per thread</span>
                    <span class="hint">Each thread gets its own copy.</span>
                  </div>
                  <Switch label="Own worktree per thread" on={trees.on} onchange={setTrees} />
                </div>
              {/if}
              {#each threads as t (t.id)}
                <button class="item" onclick={() => hub.open(t.id)} use:menu={() => threadMenu(t)}>
                  <span class="dot {t.status}" class:unread={unread(t)}></span>
                  <span class="t grow">{t.title}</span>
                  {#if t.owner}<OwnerMark owner={t.owner} />{/if}
                  <span class="m">{statusLabel[t.status]} · {t.engine} · {since(t.updated_at)}</span>
                </button>
              {:else}
                <Quiet text="No threads here" />
              {/each}
            </section>

            {#if devlog.length}
              <section>
                <div class="card-head"><h2>Devlog</h2><span class="n">last 14 days</span></div>
                {#each devlog as d (d.date)}
                  <div class="log-day" class:open={devlogOpen === d.date}>
                    <span class="log-date">{logDate(d.date)}</span>
                    <div class="log-body">
                      <button class="log-line" onclick={() => (devlogOpen = devlogOpen === d.date ? null : d.date)} data-tip={d.subjects.join("\n")}>
                        <span class="log-count">{[d.commits && `${d.commits} ${d.commits === 1 ? "commit" : "commits"}`, d.threads.length && `${d.threads.length} ${d.threads.length === 1 ? "thread" : "threads"}`].filter(Boolean).join(" · ")}</span>
                        <span class="log-subject">{d.subjects[0] ?? ""}</span>
                      </button>
                      {#if devlogOpen === d.date}
                        {#each d.subjects.slice(1) as subject}<span class="log-subject more">{subject}</span>{/each}
                        {#if d.commits > d.subjects.length}<span class="log-subject more">and {d.commits - d.subjects.length} more</span>{/if}
                      {/if}
                      {#if d.threads.length}
                        <span class="log-threads">
                          {#each d.threads as r (r.id)}
                            {@const t = hub.threads.find((t) => t.id === r.id)}
                            <button class="log-thread" onclick={() => hub.open(r.id)} data-tip="Open the thread">
                              {#if t}<span class="dot {t.status}" class:unread={unread(t)}></span>{/if}{r.title}
                            </button>
                          {/each}
                        </span>
                      {/if}
                    </div>
                  </div>
                {/each}
              </section>
            {/if}
          </div>

          <div class="col">
            <section>
              <div class="card-head">
                <h2>Running</h2>{#if runningCount}<span class="n">{runningCount}</span>{/if}
                {#if runningCount}<button class="more" onclick={() => (hub.projectTab = "services")}>Details</button>{/if}
              </div>
              <ProjectRunning kind="running" {runtime} {stopping} onstop={stop} {threadTitle} {projectOf} />
            </section>

            {#if !host}<ProjectRecipes {path} />{/if}

            {#if runtime?.uses.length}
              <section>
                <div class="card-head"><h2>Uses</h2><span class="n">{runtime.uses.length}</span></div>
                <ProjectRunning kind="uses" {runtime} {threadTitle} {projectOf} onstop={stop} />
              </section>
            {/if}

            <section>
              <div class="card-head"><h2>Repository</h2></div>
              {#if info.branch}
                <dl class="facts">
                  <dt>Branch</dt><dd class="mono">{info.branch}</dd>
                  <dt>Changes</dt><dd class:warn={!!info.changed}>{info.changed ? `${info.changed} uncommitted` : "Clean"}</dd>
                  {#if info.commits}<dt>Commits</dt><dd>{info.commits}</dd>{/if}
                </dl>
                {#if info.recent.length}
                  <div class="label sub-head">Recent commits</div>
                  {#each info.recent as c (c.hash)}
                    <div class="commit">
                      <span class="mono hash">{c.hash}</span>
                      <span class="subject" data-tip={c.subject}>{c.subject}</span>
                      <span class="m">{ago(c.ts)}</span>
                    </div>
                  {/each}
                {/if}
              {:else}
                <Quiet text="Not a git repository" />
              {/if}
            </section>

            <section>
              <div class="card-head">
                <h2>GitHub</h2>
                {#if info.github}<button class="more" onclick={() => (hub.projectTab = "github")}>Open</button>{/if}
              </div>
              {#if !info.github}
                <Quiet text="No GitHub remote" />
              {:else if !github}
                <Quiet kind="loading" />
              {:else if github.message}
                <p class="none" data-tip={github.message}>{clip(github.message, 160)}</p>
              {:else}
                {@const ci = ciState(github)}
                {#if ci && github.ci}
                  <button class="ci-row" onclick={() => openUrl(github!.ci!.url)} use:menu={() => linkMenu(github!.ci!.url)}>
                    <span class="ci {ci.cls}"></span>
                    <span class="grow"><span class="t">CI {ci.label.toLowerCase()}</span><span class="m"> · {github.ci.name}{github.ci.branch ? ` on ${github.ci.branch}` : ""}</span></span>
                  </button>
                {/if}
                <div class="label sub-head">{github.prs.length ? `Open pull requests (${github.prs.length})` : "No open pull requests"}</div>
                {#each github.prs as pr (pr.number)}
                  <button class="pr" onclick={() => openPull(pr.number)} use:menu={() => linkMenu(pr.url)}>
                    <span class="mono num">#{pr.number}</span>
                    <span class="pr-title">{pr.title}{#if pr.draft}<span class="m"> · draft</span>{/if}</span>
                  </button>
                {/each}
              {/if}
            </section>

            {#if !host}<ProjectApis {path} />{/if}

            {#if info.stack.length}
              <section>
                <div class="card-head"><h2>Stack</h2></div>
                <div class="chips">
                  {#each info.stack as s (s)}
                    <button
                      class="chip link"
                      data-tip="Open {s} in your library"
                      onclick={() => hub.openLibrary(s)}
                      use:menu={() => [{ label: `Open ${s} in the library`, run: () => hub.openLibrary(s) }, { label: `Learn ${s}`, run: () => hub.learn(s) }]}
                    >{s}</button>
                  {/each}
                </div>
              </section>
            {/if}

            <section>
              <div class="card-head"><h2>Activity</h2></div>
              <dl class="facts">
                <dt>Threads</dt><dd>{threads.length}</dd>
                {#if Number.isFinite(started)}<dt>First thread</dt><dd>{since(started)}</dd>{/if}
                {#if info.recent[0]}<dt>Last commit</dt><dd>{since(info.recent[0].ts)}</dd>{/if}
                {#if info.docs[0]}<dt>Docs updated</dt><dd>{since(info.docs[0].modified)}</dd>{/if}
                {#if stats?.answers}
                  <dt>Tokens, 30 days</dt><dd>{millions(stats.tokens)} · ≈ {money(stats.cost)}</dd>
                  <dt>Mostly</dt><dd>{stats.models[0].model}</dd>
                {/if}
              </dl>
            </section>
          </div>
        </div>
      {:else if hub.projectTab === "tests"}
        <TestTab {path} {host} focus={info.focus} onfocus={(f) => info && (info.focus = f)} onfill={(t) => composer?.fill(t, true)} onprompt={(t) => hub.startDraft(path, t, host)} />
      {:else if hub.projectTab === "rules"}
        {#key `${host}:${path}`}<RulesTab {path} {host} />{/key}
      {:else if hub.projectTab === "github"}
        <GithubTab {path} {host} onthread={startWith} />
      {:else if hub.projectTab === "files"}
        <div class="split">
          <div class="pane list">
            {#key path}<FileTree {path} {host} selected={file?.rel ?? null} onopen={openFile} />{/key}
          </div>
          <div class="pane view">
            {#if file}{@render reader(file)}{:else}<p class="none">Pick a file to read it.</p>{/if}
          </div>
        </div>
      {:else}
        <section class="defined">
          <div class="card-head">
            <h2>Starts</h2>{#if services?.services.length}<span class="n">{servicesUp}/{services.services.length}</span>{/if}
          </div>
          <p class="m hint">
            {#if services?.procfile}From <span class="mono">{services.procfile}</span>, one <span class="mono">name: command</span> per line.
            {:else if services?.services.length}Guessed from the project's manifests. <button class="link-thread more" onclick={saveGuess}>Save as Procfile.dev</button> to change them.
            {:else}Add a <span class="mono">Procfile.dev</span>, one <span class="mono">name: command</span> per line.{/if}
            {#if services?.compose} Start all also runs <span class="mono">docker compose up</span> ({services.compose}).{/if}
          </p>
          {#each services?.services ?? [] as svc (svc.name)}
            <div class="proc">
              <span class={svc.pids.length ? "live" : "dot idle"}></span>
              <span class="ports mono">{svc.ports.length ? svc.ports.map((p) => `:${p}`).join(" ") : svc.pids.length ? "running" : "stopped"}</span>
              <span class="grow">
                <span class="t">{svc.name}</span>
                <span class="cmd mono" data-tip={svc.command}>{svc.command || "no longer in the Procfile"}</span>
              </span>
              <button class="btn" class:primary={logOf === svc.name} onclick={() => (logOf = logOf === svc.name ? null : svc.name)} data-tip={svc.log}>Log</button>
              {#if svc.pids.length}
                <button class="btn danger" disabled={acting === `stop:${svc.name}`} onclick={() => runServices("stop", svc.name)}>Stop</button>
              {:else if svc.command}
                <button class="btn" disabled={acting === `start:${svc.name}`} onclick={() => runServices("start", svc.name)}>Start</button>
              {/if}
            </div>
            {#if logOf === svc.name}
              <pre class="code log">{logText || "Nothing written yet."}</pre>
            {/if}
          {/each}
        </section>
        <div class="services">
          <section>
            <div class="card-head"><h2>Running</h2>{#if runningCount}<span class="n">{runningCount}</span>{/if}</div>
            <ProjectRunning kind="running" full {runtime} {stopping} onstop={stop} {threadTitle} {projectOf} />
          </section>
          <section>
            <div class="card-head"><h2>Uses</h2><span class="n">{runtime?.uses.length ?? 0}</span></div>
            <p class="m hint">Not started by this project.</p>
            <ProjectRunning kind="uses" {runtime} {threadTitle} {projectOf} onstop={stop} />
            {#if runtime && !runtime.uses.length}<Quiet text="Nothing outside the project" />{/if}
          </section>
        </div>
      {/if}
    </div>
  </div>
  {/if}

  <div class="dock">
    <Composer
      bind:this={composer}
      placeholder={`What should we do in ${info?.name ?? folderName(path)}?`}
      {engine}
      {session}
      onengine={pickEngine}
      cwd={path}
      onsubmit={start}
      ontyping={() => warm.typing(startOf())}
      onsetting={setting}
      host={host}
      findFiles={(q) => api.filesIn(path, q, host)}
      onerror={(e) => hub.fail(e)}
    />
  </div>
</div>

<style>
  /* @kit projectView */
</style>
