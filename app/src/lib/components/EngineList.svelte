<!-- Settings > Engines: the harnesses (the agents: Claude Code, Codex, Hermes, OpenCode, Gemini CLI) and adding one. A harness that runs on many providers is added once; its threads pick the provider and model in the model picker, from the providers with a key (`routes.rs`). Adding one writes it to hub.json (`engines.rs`); it runs after the core restarts.
     Used by Settings.svelte. -->
<script lang="ts">
  import { quiet, quietly } from "../quiet";
  import { poll } from "../poll";
  import Quiet from "./Quiet.svelte";
  // Settings > Engines: the harnesses (the agents: Claude Code, Codex, Hermes, OpenCode, Gemini
  // CLI) and adding one. A harness that runs on many providers is added once; its threads pick
  // the provider and model in the model picker, from the providers with a key (`routes.rs`).
  // Adding one writes it to hub.json (`engines.rs`); it runs after the core restarts.
  import Working from "./Working.svelte";
  import { api } from "../api";
  import type { EngineCatalog } from "../bindings/EngineCatalog";
  import type { EngineDraft } from "../bindings/EngineDraft";
  import type { EngineEntry } from "../bindings/EngineEntry";
  import type { EngineTest } from "../bindings/EngineTest";
  import type { InstallJob } from "../bindings/InstallJob";
  import type { MachineEngines } from "../bindings/MachineEngines";
  import { openUrl } from "../native";
  import { hub } from "../state.svelte";
  import { hubdHas } from "../compat";
  import Field from "./Field.svelte";
  import Select from "./Select.svelte";
  import HarnessSettings from "./HarnessSettings.svelte";
  import Icon from "./Icon.svelte";
  import ProviderMark from "./ProviderMark.svelte";
  import { engineProvider, machineShort } from "../providers.svelte";
  import type { UsageReport } from "../bindings/UsageReport";
  import type { PlanLimit } from "../bindings/PlanLimit";

  let { onchange }: { onchange?: () => void } = $props();

  /** Engines are added on this machine and sent to the others from here (`engines.rs`), so the
   * page always shows this one; each engine says how it stands on the others. */
  const host = "";
  let machines = $state<MachineEngines[]>([]);
  /** Installs running on another machine, by "machine/harness". */
  let remoteInstalls = $state<Record<string, InstallJob>>({});
  let catalog = $state<EngineCatalog | null>(null);
  let failed = $state("");
  let adding = $state(false);
  let draft = $state<EngineDraft>(blank());
  let testing = $state(false);
  let tested = $state<EngineTest | null>(null);
  let saving = $state(false);
  let removing = $state("");
  /** The engine whose harness settings are open under its row. */
  let open = $state("");
  let restarting = $state(false);
  let testingRow = $state("");
  let rowTests = $state<Record<string, EngineTest>>({});

  function blank(): EngineDraft {
    return { name: "", harness: "", provider: "", model: "", key: "", key_name: "", command: "", provider_id: "", secret: "" };
  }

  const harness = $derived(catalog?.harnesses.find((h) => h.id === draft.harness) ?? null);
  /** The providers a harness runs on beside its own login (Claude Code on DeepSeek); a hubd older
   *  than `engines.pairs` sends none. */
  const pairsOf = (h: { id: string } | null | undefined): string[] => (h as { pairs?: string[] } | null | undefined)?.pairs ?? [];
  const providers = $derived(catalog?.providers.filter((p) => harness?.providers.includes(p.id) || pairsOf(harness).includes(p.id)) ?? []);
  const provider = $derived(catalog?.providers.find((p) => p.id === draft.provider) ?? null);
  /** A key has to be pasted: the provider takes one and none from the vault is picked. */
  const needsKey = $derived(
    draft.provider === "other" ? !!draft.secret.trim() : !!provider && !draft.key_name,
  );
  const suggestions = $derived([
    ...new Set([...(provider?.models ?? []), ...(tested?.models.map(([v]) => v) ?? [])]),
  ]);
  /** Each harness is there once; its threads pick their provider. */
  const addable = $derived(catalog?.harnesses.filter((h) => !catalog!.engines.some((e) => e.harness === h.id || e.name === h.id)) ?? []);
  /** A harness that is there already, on a provider's key beside its login (Claude Code on DeepSeek),
   *  offered once the vault has the key. In the model picker it shows under the provider's tab, as
   *  one more harness for the same models. */
  const pairable = $derived(
    (catalog?.harnesses ?? []).flatMap((h) =>
      pairsOf(h).flatMap((id) => {
        const p = catalog!.providers.find((x) => x.id === id);
        return p?.has_key && !catalog!.engines.some((e) => e.harness === h.id && e.provider === p.id) ? [{ h, p }] : [];
      }),
    ),
  );
  let pairsHere = $state<boolean | null>(null);
  quiet(hubdHas("engines.pairs").then((v) => (pairsHere = v)), "hubd has"); // compat-until: 2026-12-02
  /** Pairs exist, but this hubd is older than the feature: say so rather than show nothing. */
  const pairsWait = $derived(pairsHere === false && !!catalog?.providers.some((p) => p.id === "deepseek" && p.has_key));
  function pickPair(h: { id: string }, p: { id: string; models: string[]; keys: { name: string }[] }) {
    draft = { ...blank(), harness: h.id, provider: p.id, model: p.models[0] ?? "", key_name: p.keys[0]?.name ?? "" };
    tested = null;
  }
  /** The providers a harness's threads can pick from: the ones it runs on that have a key. */
  function runsOn(id: string | null) {
    const h = catalog?.harnesses.find((x) => x.id === id);
    return catalog?.providers.filter((p) => h?.providers.includes(p.id) && p.has_key) ?? [];
  }
  const waiting = $derived(catalog?.engines.filter((e) => !e.running) ?? []);
  const autoName = $derived(
    [draft.harness === "other" ? "" : draft.harness, draft.provider === "other" ? draft.provider_id : draft.provider]
      .filter(Boolean)
      .join("-")
      .toLowerCase()
      .replace(/[^a-z0-9]+/g, "-")
      .replace(/^-|-$/g, ""),
  );

  async function load() {
    failed = "";
    try {
      catalog = await api.engineCatalog(host || null);
      quiet(api.engineMachines().then((m) => (machines = m)), "engine machines");
    } catch (e) {
      catalog = null;
      failed = e instanceof Error ? e.message : String(e);
    }
  }

  $effect(() => {
    void host;
    catalog = null;
    adding = false;
    jobs = {};
    stop();
    load();
    // An install started earlier (or from another window) shows where it is.
    api.installJobs(host || null).then((list) => {
      jobs = Object.fromEntries(list.map((j) => [j.harness, j]));
      if (list.some((j) => j.running)) follow();
    }).catch(quietly("install jobs"));
  });

  /** Installs on the shown machine, by harness, while they run and after. */
  let jobs = $state<Record<string, InstallJob>>({});
  let following: (() => void) | null = null;

  async function install(id: string) {
    try {
      const job = await api.installHarness(id, host || null);
      jobs = { ...jobs, [id]: job };
      follow();
    } catch (e) {
      hub.fail(e);
    }
  }

  /** Reads the installs every second while one runs; when one is done, the catalog again. */
  function follow() {
    if (following) return;
    following = poll(async () => {
      try {
        const list = await api.installJobs(host || null);
        const before = jobs;
        jobs = Object.fromEntries(list.map((j) => [j.harness, j]));
        const finished = list.filter((j) => !j.running && before[j.harness]?.running);
        if (finished.length) {
          await load();
          // Installed for the engine being added: try it straight away when it is ready to try.
          if (finished.some((j) => j.ok && j.harness === draft.harness) && (!needsKey || draft.key.trim())) test();
        }
        if (!list.some((j) => j.running)) stop();
      } catch {
        // The core may be restarting; keep asking.
      }
    }, 1_000);
  }

  function stop() {
    if (following) following();
    following = null;
  }

  $effect(() => stop);

  function since(at: number): string {
    const s = Math.max(0, Math.round((Date.now() - at) / 1000));
    return s < 60 ? `${s} s` : `${Math.floor(s / 60)} min ${s % 60} s`;
  }

  function pickHarness(id: string) {
    // Named after the harness alone: its threads pick the provider.
    draft = { ...blank(), harness: id, name: id === "other" ? "" : id };
    tested = null;
  }

  function pickProvider(id: string) {
    const p = catalog?.providers.find((x) => x.id === id);
    // A key already in the vault for it is picked straight away (its own name first, then names
    // like DEEPSEEK); another one or a new one can still be chosen.
    draft = { ...draft, provider: id, model: p?.models[0] ?? "", key: "", key_name: p?.keys[0]?.name ?? "", provider_id: "", secret: "" };
    tested = null;
  }

  async function test() {
    testing = true;
    tested = null;
    try {
      tested = await api.testEngine(draft, host || null);
    } catch (e) {
      hub.fail(e);
    } finally {
      testing = false;
    }
  }

  async function add() {
    saving = true;
    try {
      catalog = await api.addEngine(draft, host || null);
      shareNow();
      adding = false;
      draft = blank();
      tested = null;
      onchange?.();
    } catch (e) {
      hub.fail(e);
    } finally {
      saving = false;
    }
  }

  async function remove(e: EngineEntry) {
    if (removing !== e.name) {
      removing = e.name;
      return;
    }
    try {
      catalog = await api.removeEngine(e.name, host || null);
      shareNow();
      onchange?.();
    } catch (err) {
      hub.fail(err);
    } finally {
      removing = "";
    }
  }

  async function testRow(e: EngineEntry) {
    testingRow = e.name;
    try {
      // The saved engine is tried as it would be added again: same harness, provider and model.
      const d: EngineDraft = {
        ...blank(),
        name: e.name,
        harness: e.harness ?? "other",
        command: e.harness ? "" : e.command,
        provider: e.provider && catalog?.providers.some((p) => p.id === e.provider) ? e.provider : e.provider ? "other" : "",
        provider_id: e.provider ?? "",
        secret: e.secrets[0]?.[0] ?? "",
        key_name: e.secrets[0]?.[0] ?? "",
        model: bareModel(e),
      };
      if (e.harness && catalog?.harnesses.find((h) => h.id === e.harness)?.command !== e.command) d.command = e.command;
      rowTests = { ...rowTests, [e.name]: await api.testEngine(d, host || null) };
    } catch (err) {
      hub.fail(err);
    } finally {
      testingRow = "";
    }
  }

  /** The model as picked, without the provider the harness puts in front. */
  function bareModel(e: EngineEntry): string {
    const m = e.model ?? "";
    const p = e.provider;
    if (!p) return m;
    return m.startsWith(`${p}:`) || m.startsWith(`${p}/`) ? m.slice(p.length + 1) : m;
  }

  async function editCommand(e: EngineEntry, command: string) {
    try {
      await api.editConfig({ key: `engines.${e.name}`, value: command || null }, host || null);
      await load();
      onchange?.();
    } catch (err) {
      hub.fail(err);
    }
  }

  async function restart() {
    restarting = true;
    try {
      await api.restartCore(host || null);
      // The core comes back within a few seconds; read again once it answers.
      for (let i = 0; i < 30; i++) {
        await new Promise((r) => setTimeout(r, 1_000));
        try {
          catalog = await api.engineCatalog(host || null);
          if (!catalog.engines.some((e) => !e.running)) break;
        } catch {
          // Still restarting.
        }
      }
      onchange?.();
    } finally {
      restarting = false;
    }
  }

  /** "Starts on DeepSeek Reasoner, of 2 models." */
  function started(t: EngineTest): string {
    const name = t.models.find(([v]) => v === t.current)?.[1] || t.current;
    const of = `${t.models.length} model${t.models.length === 1 ? "" : "s"}`;
    return name ? `Starts on ${name}, of ${of}.` : `Offers ${of}.`;
  }

  /** Sends the engines to the other machines now and reads how they stand there. */
  async function shareNow() {
    if (!hub.hosts.some((h) => !h.local)) return;
    try {
      machines = await api.shareEngines();
    } catch {
      // The next minute's send tries again.
    }
  }

  /** How engine `e` stands on machine `m`, in a few words, and whether Install fits. */
  function onMachine(e: EngineEntry, m: MachineEngines): { text: string; ok: boolean; install: boolean; bad?: boolean } {
    if (m.error) return { text: m.error, ok: false, install: false };
    const there = m.engines.find((x) => x.name === e.name);
    if (!there) return { text: m.at ? "being sent" : "not sent yet", ok: false, install: false };
    if (!there.installed) {
      const h = catalog?.harnesses.find((x) => x.id === there.harness);
      return { text: "not installed", ok: false, install: !!h?.install };
    }
    // Ready only once it was really tried there: the agent started and the provider took the key.
    if (!there.can_try) return { text: "installed, but its hubd is too old to try it; update it", ok: false, install: false, bad: true };
    if (!there.running) return { text: "restarting to take it in…", ok: false, install: false };
    const c = there.check;
    if (!c) return { text: there.checking ? "trying it there…" : "waits to be tried there", ok: false, install: false };
    const again = there.checking ? " · trying again…" : "";
    return { text: (c.ok ? `ready · ${c.message}` : c.message) + again, ok: c.ok, install: false, bad: !c.ok };
  }

  /** The mono line under an engine's name: its id and what it runs on, short. */
  function sub(e: EngineEntry): string {
    const h = catalog?.harnesses.find((x) => x.id === e.harness);
    if (h?.providers.length) return `${e.name} · ${paidBy(e) ?? (h.own_login ? "own setup" : "no provider yet")}`;
    const p = e.provider ? (catalog?.providers.find((x) => x.id === e.provider)?.label ?? e.provider) : "own login";
    return [e.name, p, e.model].filter(Boolean).join(" · ");
  }

  /** "on DeepSeek" when an engine runs on a keyed provider, else nothing: it uses its own login. */
  function paidBy(e: EngineEntry): string | null {
    const h = catalog?.harnesses.find((x) => x.id === e.harness);
    if (!h?.providers.length) return e.provider ? "on " + (catalog?.providers.find((x) => x.id === e.provider)?.label ?? e.provider) : null;
    const keyed = runsOn(h.id).filter((p) => !e.provider || p.id === e.provider).map((p) => p.label);
    return keyed.length ? `on ${keyed.join(", ")}` : null;
  }

  /** The engine open to its command and buttons. */
  let details = $state("");

  /** One lamp per machine: each one's own answer for a shared engine, or whether its harness is
   * here for one every machine has (Claude Code, Codex). */
  function lights(e: EngineEntry): { m: string; st: ReturnType<typeof onMachine>; job?: InstallJob }[] {
    if (e.shared_from) return [];
    const here = hub.hosts.find((h) => h.local)?.name ?? "This machine";
    const installed = catalog?.harnesses.find((h) => h.id === (e.harness ?? e.name))?.installed ?? true;
    const out: { m: string; st: ReturnType<typeof onMachine>; job?: InstallJob }[] = [{ m: here, st: { text: installed ? (e.running ? "ready" : "starts after a restart") : "not installed", ok: installed && e.running, install: false, bad: !installed } }];
    for (const m of machines) {
      // An added engine is sent to every machine, so each one has a lamp; one every machine has
      // (Claude Code, Codex) only shows where that machine reported it.
      if (!e.builtin || m.engines.some((x) => x.name === e.name)) out.push({ m: m.machine, st: onMachine(e, m), job: remoteInstalls[`${m.machine}/${e.harness}`] });
    }
    return out;
  }

  /** How much of its plan an engine has used, from the limits Hub already reads (`usage.rs`). */
  let usage = $state<UsageReport | null>(null);
  $effect(() => {
    quiet(api.usage(false).then((u) => (usage = u)), "usage");
  });
  function gauge(e: EngineEntry): { label: string; percent: number } | null {
    const kind = e.harness ?? e.name;
    const plan = kind === "claude" ? usage?.plan : usage?.others.find((o) => o.engine === e.name || o.engine === kind)?.plan;
    const top = plan?.limits.reduce<PlanLimit | null>((a, l) => (!a || l.percent > a.percent ? l : a), null);
    return top ? { label: top.label, percent: Math.round(top.percent) } : null;
  }

  async function installOn(machine: string, harness: string) {
    const key = `${machine}/${harness}`;
    try {
      remoteInstalls = { ...remoteInstalls, [key]: await api.installHarness(harness, machine) };
      // Follow it there until it is done, then send the engines again so its command is found.
      for (;;) {
        await new Promise((r) => setTimeout(r, 2_000));
        const job = (await api.installJobs(machine)).find((j) => j.harness === harness);
        if (!job) break;
        remoteInstalls = { ...remoteInstalls, [key]: job };
        if (!job.running) break;
      }
      await shareNow();
    } catch (err) {
      hub.fail(err);
    }
  }


</script>

{#if failed}
  <div class="empty">Could not read the engines{host ? ` on ${host}` : ""}: {failed}</div>
{:else if !catalog}
  <Quiet kind="loading" />
{:else}
  <div class="rack">
  {#each catalog.engines as e (e.name)}
    {@const t = rowTests[e.name]}
    {@const states = lights(e)}
    {@const trouble = states.filter(({ st, job }) => !st.ok || job)}
    {@const meter = gauge(e)}
    {@const isOpen = details === e.name}
    <div class="rack-row" class:open={isOpen}>
      <button class="rack-face face" aria-expanded={isOpen} onclick={() => (details = isOpen ? "" : e.name)}>
        <span class="rack-badge"><ProviderMark id={engineProvider(e.name).id} size={20} /></span>
        <span class="rack-main">
          <span class="rack-name">
            {e.label}
            {#if e.default}<span class="pill">Default</span>{/if}
            {#if !e.running}<span class="pill warn" data-tip="Starts after a restart">After restart</span>{/if}
            {#each e.secrets.filter(([, has]) => !has) as [name] (name)}<span class="pill warn">No {name}</span>{/each}
          </span>
          <span class="rack-sub">{sub(e)}</span>
        </span>
        <span class="leds">
          {#each states as { m, st, job } (m)}
            <span class="led-box" data-tip={`${m}: ${job?.running ? "installing…" : st.text}`}>
              <span class="lamp" class:on={st.ok && !job} class:bad={st.bad || job?.ok === false} class:busy={job?.running}></span>{machineShort(m)}
            </span>
          {/each}
        </span>
        <span class="meter">
          {#if meter}
            <span class="mrow"><span>{meter.label}</span><span>{meter.percent}%</span></span>
            <span class="bar"><i style:width="{Math.min(100, meter.percent)}%" class:hot={meter.percent >= 85}></i></span>
          {:else}
            <span class="mrow"><span>{paidBy(e) ? "Pays per use" : catalog?.harnesses.find((x) => x.id === e.harness)?.providers.length ? "Own setup" : "Subscription"}</span></span>
          {/if}
        </span>
        <span class="chev"><Icon name="chevron" size={12} /></span>
      </button>
      {#if e.shared_from || trouble.length || t || isOpen}
        <div class="rack-more">
          {#if e.shared_from}
            <span class="dim">Added on {e.shared_from}; change it there.</span>
          {:else if trouble.length}
            <!-- Only what is wrong gets words; the lamps say the rest. -->
            <span class="trouble">
              {#each trouble as { m, st, job } (m)}
                <span class="machine">
                  <span class="m">{m}</span>
                  {#if job?.running}<span class="dim">installing… {job.lines.at(-1) ?? ""}</span>
                  {:else if job?.ok === false}<span class="bad">{job.message}</span>
                  {:else if job?.ok}<span class="dim">installed</span>
                  {:else}<span class="dim" class:bad={st.bad}>{st.text}</span>{/if}
                  {#if st.install && !job?.running}<button class="link" onclick={() => installOn(m, e.harness!)}>Install</button>{/if}
                </span>
              {/each}
            </span>
          {/if}
          {#if t}
            <div class="result" class:bad={!t.ok}>
              {t.message}{#if t.agent} <span class="dim">({t.agent})</span>{/if}
              {#if t.ok && t.models.length}<div class="dim">{started(t)}</div>{/if}
              {#if t.fix}<div class="fix mono">{t.fix}</div>{/if}
            </div>
          {/if}
          {#if isOpen}
            <div class="controls">
              <Field value={e.command} width={360} onsave={(v) => editCommand(e, v)} />
              {#if e.harness && catalog.harnesses.find((h) => h.id === e.harness)?.settings}
                <button class="btn" class:active={open === e.name} aria-expanded={open === e.name} onclick={() => (open = open === e.name ? "" : e.name)}>Settings</button>
              {/if}
              <button class="btn" disabled={testingRow === e.name} onclick={() => testRow(e)}>{testingRow === e.name ? "Trying…" : "Try"}</button>
              {#if !e.default && (!e.builtin || e.command !== catalog.harnesses.find((h) => h.id === e.name)?.command)}
                <button class="btn danger" onclick={() => remove(e)} onblur={() => (removing = "")}>
                  {removing === e.name ? (e.builtin ? "Reset?" : "Remove?") : e.builtin ? "Reset" : "Remove"}
                </button>
              {/if}
            </div>
            {#if open === e.name && e.harness}
              <HarnessSettings harness={e.harness} {host} />
            {/if}
          {/if}
        </div>
      {/if}
    </div>
  {/each}
  {#if !adding}
    <button class="rack-add" onclick={() => { adding = true; draft = blank(); tested = null; }}><Icon name="plus" size={12} /> Add harness</button>
  {/if}
  </div>

  {#if waiting.length && host}
    <div class="line">
      <div class="what"><div class="hint">{waiting.map((e) => e.name).join(", ")} start{waiting.length === 1 ? "s" : ""} after the core {host ? `on ${host} ` : ""}restarts.</div></div>
      <div class="control"><button class="btn primary" disabled={restarting} onclick={restart}>{restarting ? "Restarting…" : "Restart core"}</button></div>
    </div>
  {/if}

  {#if adding}
    <div class="add">
      <div class="step">
        <div class="label">Harness <span class="dim">the agent that does the work</span></div>
        <div class="chips">
          {#each addable as h (h.id)}
            <button class="chip" class:active={draft.harness === h.id} data-tip={h.what} onclick={() => pickHarness(h.id)}>
              {h.label}{#if !h.installed}<span class="dim">&nbsp;· not installed</span>{/if}
            </button>
          {/each}
          {#each pairable as { h, p } (h.id + p.id)}
            <button class="chip" class:active={draft.harness === h.id && draft.provider === p.id} data-tip="{h.label} on {p.label}'s key" onclick={() => pickPair(h, p)}>
              {h.label}<span class="dim">&nbsp;· on {p.label}'s key</span>
            </button>
          {/each}
          <button class="chip" class:active={draft.harness === "other"} data-tip="Any agent that speaks ACP" onclick={() => pickHarness("other")}>Other</button>
        </div>
        {#if pairsWait}<Quiet kind="newer" />{/if}
        {#if harness}
          <div class="hint">{harness.what}</div>
          {#if !harness.installed && harness.install}
            {@const job = jobs[harness.id]}
            <div class="install" class:bad={job?.ok === false}>
              {#if job?.running}
                <div class="row"><Working /><span>Installing {harness.label} on {host || "this machine"}…</span><span class="dim">{since(job.started)}</span></div>
              {:else if job?.ok === false}
                <div class="row"><span>{job.message}</span><button class="btn" onclick={() => install(harness.id)}>Try again</button></div>
              {:else}
                <div class="row">
                  <span>{harness.label} is not on {host || "this machine"} yet.</span>
                  <button class="btn primary" onclick={() => install(harness.id)}>Install {harness.label}</button>
                </div>
                <div class="dim small">Runs its own installer: <span class="mono">{harness.install}</span></div>
              {/if}
              {#if job && (job.running || job.ok === false) && job.lines.length}
                <pre class="log">{job.lines.slice(-8).join("\n")}</pre>
              {/if}
            </div>
          {:else if jobs[harness.id]?.ok}
            <div class="hint ok">{jobs[harness.id].message}</div>
          {/if}
        {:else if draft.harness === "other"}
          <input class="field mono" placeholder="The command that starts it as an ACP agent, e.g. my-agent --acp" bind:value={draft.command} spellcheck="false" />
        {/if}
      </div>

      {#if draft.harness}
        <div class="step">
          <div class="label">{harness?.providers.length ? "New threads start on" : "Provider"} <span class="dim">{harness?.providers.length ? "each thread can pick another in the model picker" : "where the model comes from"}</span></div>
          <div class="chips">
            {#if !harness || harness.own_login}
              <button class="chip" class:active={draft.provider === ""} data-tip={harness?.login ? `Log in once with: ${harness.login}` : "What the agent is logged in to"} onclick={() => pickProvider("")}>{harness?.providers.length ? "Its own setup" : "Own login"}</button>
            {/if}
            {#each harness?.providers.length ? providers.filter((p) => p.has_key) : providers as p (p.id)}
              <button class="chip" class:active={draft.provider === p.id} data-tip={p.what} onclick={() => pickProvider(p.id)}>
                {p.label}{#if p.has_key}<span class="dim">&nbsp;· key saved</span>{/if}
              </button>
            {/each}
            {#if !harness}
              <button class="chip" class:active={draft.provider === "other"} data-tip="A provider the list lacks, by the name the harness knows it" onclick={() => pickProvider("other")}>Other</button>
            {/if}
          </div>
          {#if harness?.providers.length && !providers.some((p) => p.has_key)}
            <div class="hint">Add a key under Providers below.</div>
          {:else if provider}
            <div class="hint">{provider.what}</div>
          {:else if draft.provider === "other"}
            <div class="row">
              <input class="field mono" placeholder="Its id in the harness, e.g. kimi-coding" bind:value={draft.provider_id} spellcheck="false" />
              <input class="field mono" placeholder="Key name, e.g. KIMI_API_KEY" bind:value={draft.secret} spellcheck="false" />
            </div>
          {/if}
          {#if provider && provider.keys.length}
            <div class="row">
              <span class="label-inline">Key</span>
              <Select mono class="pick" label="Key" bind:value={draft.key_name} onchange={() => { draft.key = ""; tested = null; }}
                options={[...provider.keys.map((k) => ({ value: k.name, label: k.name, hint: k.hint })), { value: "", label: "Paste a new key…" }]} />
            </div>
            {#if draft.key_name}<div class="hint"><span class="mono">{provider.secret}</span></div>{/if}
          {/if}
          {#if needsKey}
            <div class="row">
              <input class="field mono" type="password" placeholder="Paste the key; it goes to the key vault" bind:value={draft.key} spellcheck="false" autocomplete="off" />
              {#if provider?.keys_at}<button class="link small" onclick={() => openUrl(provider!.keys_at!)}>Get a key</button>{/if}
            </div>
            <div class="hint">Paid per token to {provider?.label ?? "the provider"}.</div>
          {/if}
        </div>

        {#if draft.provider}
          <div class="step">
            <div class="label">Model <span class="dim">what new threads start on; you can switch in the thread</span></div>
            <input class="field mono" list="engine-models" placeholder="e.g. deepseek-chat" bind:value={draft.model} spellcheck="false" />
            <datalist id="engine-models">{#each suggestions as m (m)}<option value={m}></option>{/each}</datalist>
          </div>
        {/if}

        {#if draft.harness === "other"}
          <div class="step">
            <div class="label">Name <span class="dim">in the model picker and hub.json</span></div>
            <input class="field mono narrow" placeholder={autoName || "my-engine"} bind:value={draft.name} spellcheck="false" />
          </div>
        {/if}

        {#if tested}
          <div class="result" class:bad={!tested.ok}>
            {tested.message}{#if tested.agent} <span class="dim">({tested.agent})</span>{/if}
            {#if tested.ok && tested.models.length}<div class="dim">{started(tested)}</div>{/if}
            {#if tested.fix}<div class="fix mono">{tested.fix}</div>{/if}
            {#if tested.ok && !tested.model_found && tested.models.length}
              <div class="chips models">
                {#each tested.models as [value, name] (value)}
                  <button class="chip" onclick={() => { draft.model = value; tested = null; }}>{name || value}</button>
                {/each}
              </div>
            {/if}
          </div>
        {/if}

        <div class="buttons">
          <button class="btn" disabled={testing || (draft.harness === "other" && !draft.command.trim())} onclick={test}>{testing ? "Trying… (up to two minutes)" : "Try it"}</button>
          <button class="btn primary" disabled={saving || (needsKey && !draft.key.trim())} onclick={add}>{saving ? "Adding…" : "Add"}</button>
          <button class="btn" onclick={() => { adding = false; tested = null; }}>Cancel</button>
        </div>
      {:else}
        <div class="buttons"><button class="btn" onclick={() => (adding = false)}>Cancel</button></div>
      {/if}
    </div>
  {/if}
{/if}

<style>
  .face { grid-template-columns: 48px minmax(0, 1fr) auto 190px 26px; }
  .face:hover { background: color-mix(in srgb, var(--surface-hover) 45%, transparent); }
  .leds { display: flex; gap: 14px; padding: 0 14px; }
  .meter { padding: 0 14px; display: flex; flex-direction: column; gap: 4px; }
  .mrow { display: flex; justify-content: space-between; gap: 8px; font-size: 12px; color: var(--muted); white-space: nowrap; }
  .bar { height: 4px; border-radius: 2px; background: var(--surface-hover); position: relative; overflow: hidden; }
  .bar i { position: absolute; inset: 0 auto 0 0; border-radius: 2px; background: var(--text-3); }
  .bar i.hot { background: var(--amber); }
  .chev { color: var(--muted); display: flex; transition: transform var(--dur-fast) var(--ease); }
  .rack-row.open .chev { transform: rotate(90deg); }
  .trouble { display: flex; flex-wrap: wrap; gap: 4px 18px; }
  .trouble .machine { gap: 8px; }
  .trouble .m { color: var(--text); }
  .controls { display: flex; align-items: center; gap: 8px; flex-wrap: wrap; }
  .line { display: flex; align-items: center; gap: 32px; padding: 14px 0; }
  .what { flex: 1; min-width: 0; display: flex; flex-direction: column; gap: 4px; }
  .hint { color: var(--muted); font-size: 13px; }
  .control { display: flex; align-items: center; gap: 8px; flex: none; }
  .dim { color: var(--muted); }
  .pill { font-size: 12px; color: var(--text-3); background: var(--surface-hover); border-radius: var(--r-sm); padding: 1px 8px; white-space: nowrap; }
  .pill.warn { color: var(--amber); background: var(--amber-bg); }
  .result { font-size: 13px; padding: 6px 10px; border-radius: var(--r-md); color: var(--green); background: var(--green-bg); margin-top: 4px; }
  .result.bad { color: var(--red); background: var(--red-bg); }
  .fix { margin-top: 4px; color: var(--text-2); user-select: text; }
  .add { display: flex; flex-direction: column; gap: 14px; padding: 14px 0 4px; }
  .step { display: flex; flex-direction: column; gap: 6px; }
  .label { color: var(--text); font-size: 13px; }
  .label .dim { font-size: 12px; margin-left: 4px; }
  .chips { display: flex; flex-wrap: wrap; gap: 6px; }
  .chips.models { margin-top: 6px; }
  .chip { height: 26px; padding: 0 10px; border-radius: var(--r-md); border: 1px solid var(--line-strong); color: var(--text-2); font-size: 13px; }
  .chip:hover { color: var(--text); background: var(--surface-hover); }
  .chip.active { border-color: var(--accent); color: var(--text); background: var(--accent-bg); }
  .row { display: flex; gap: 8px; align-items: center; }
  .field { height: 28px; padding: 0 8px; background: var(--bg); border: 1px solid var(--line-strong); border-radius: var(--r-md); font-size: 13px; outline: none; color: var(--text); width: 100%; max-width: 420px; }
  .field:focus { border-color: var(--accent); }
  .field.narrow { max-width: 200px; }
  .row :global(.select.pick) { flex: 1; max-width: 320px; }
  .label-inline { color: var(--text-2); font-size: 13px; }
  .link { color: var(--accent); font-size: inherit; padding: 0; text-align: left; }
  .link.small { font-size: 13px; white-space: nowrap; }
  .buttons { display: flex; gap: 8px; }
  .empty { color: var(--muted); padding: 12px 0; font-size: 13px; }
  .machine { display: inline-flex; align-items: center; gap: 6px; max-width: 100%; }
  .machine .dim { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; max-width: 320px; }
  .machine .bad { color: var(--red); }
  .install { display: flex; flex-direction: column; gap: 6px; padding: 10px 12px; border-radius: var(--r-md); background: var(--surface); color: var(--text-2); font-size: 13px; max-width: 720px; }
  .install.bad { color: var(--red); background: var(--red-bg); }
  .install .row { gap: 10px; flex-wrap: wrap; }
  .small { font-size: 12px; }
  .hint.ok { color: var(--green); }
  .log { margin: 0; padding: 6px 8px; background: var(--bg); border-radius: var(--r-sm); font-size: 12px; line-height: 1.45; color: var(--text-3); white-space: pre-wrap; word-break: break-all; max-height: 150px; overflow: auto; user-select: text; }
</style>
