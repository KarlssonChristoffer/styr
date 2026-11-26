<!-- The Problems page's three settings panels: Guard (on, what it reads; one Guard reads every machine's problems, from the machine that has the others), rules (global TOML, the projects' own files, what Guard suggests), and connecting a program (Sentry DSN or OpenTelemetry endpoint, when it counts as quiet, its cron jobs).
     Used by ProblemsView.svelte. -->
<script lang="ts">
  import { quiet } from "../quiet";
  // The Problems page's three settings panels: Guard (on, what it reads; one Guard reads every
  // machine's problems, from the machine that has the others),
  // rules (global TOML, the projects' own files, what Guard suggests), and connecting a
  // program (Sentry DSN or OpenTelemetry endpoint, when it counts as quiet, its cron jobs).
  import { onMount } from "svelte";
  import { api } from "../api";
  import type { Monitor } from "../bindings/Monitor";
  import type { ProblemRules } from "../bindings/ProblemRules";
  import type { Source } from "../bindings/Source";
  import type { VaktenStatus } from "../bindings/VaktenStatus";
  import { ago } from "../format";
  import { hub } from "../state.svelte";
  import Icon from "./Icon.svelte";
  import Select from "./Select.svelte";
  import Switch from "./Switch.svelte";

  let { which, vakten = $bindable() }: { which: "vakten" | "rules" | "connect"; vakten: VaktenStatus | null } = $props();

  let rules = $state<ProblemRules | null>(null);
  let text = $state("");
  let sources = $state<Source[]>([]);
  let monitors = $state<Monitor[]>([]);
  let newName = $state("");
  let newFolder = $state("");

  onMount(() => {
    if (which === "rules") api.problemRules().then((r) => ((rules = r), (text = r.global))).catch((e) => hub.fail(e));
    if (which === "connect") {
      quiet(api.problemSources().then((s) => (sources = s)), "problem sources");
      quiet(api.problemMonitors().then((m) => (monitors = m)), "problem monitors");
    }
  });

  async function setVakten(change: Partial<VaktenStatus["prefs"]>) {
    if (!vakten) return;
    try {
      vakten = await api.setVakten({ ...vakten.prefs, ...change });
    } catch (e) {
      hub.fail(e);
    }
  }

  async function look() {
    try {
      vakten = await api.vaktenLook();
      hub.tell(vakten.state === "working" ? "Guard is looking" : "Nothing new for Guard");
    } catch (e) {
      hub.fail(e);
    }
  }

  async function saveRules() {
    try {
      rules = await api.setProblemRules(text);
      hub.tell("Rules saved");
    } catch (e) {
      hub.fail(e);
    }
  }

  async function addRule(i: number) {
    if (!rules) return;
    try {
      rules = await api.addProblemRule(rules.suggestions[i].rule);
      text = rules.global;
    } catch (e) {
      hub.fail(e);
    }
  }

  async function addSource() {
    try {
      const s = await api.addProblemSource({ name: newName, folder: newFolder || undefined });
      sources = [...sources.filter((x) => x.id !== s.id), s];
      newName = "";
      newFolder = "";
    } catch (e) {
      hub.fail(e);
    }
  }

  async function quietAfter(s: Source, hours: number) {
    try {
      const n = await api.addProblemSource({ name: s.name, quiet_after_hours: hours });
      sources = sources.map((x) => (x.id === n.id ? n : x));
    } catch (e) {
      hub.fail(e);
    }
  }

  async function removeSource(s: Source) {
    try {
      await api.removeProblemSource(s.id);
      sources = sources.filter((x) => x.id !== s.id);
    } catch (e) {
      hub.fail(e);
    }
  }

  function copy(t: string) {
    navigator.clipboard.writeText(t).then(() => hub.tell("Copied")).catch((e) => hub.fail(e));
  }
</script>

<section class="panel">
  {#if which === "vakten" && vakten}
    <p class="say">
      One Guard looks after every machine online. It sorts problems, hands fixable ones to fix threads that end at a pull request, and leaves ask-first ones to you.
    </p>
    <div class="grid">
      <label class="check"><Switch label="Guard is on" on={vakten.prefs.on} onchange={(on) => setVakten({ on })} /> Guard is on</label>
      <label>Reads by itself
        <Select value={vakten.prefs.level} onchange={(level) => setVakten({ level })}
          options={[{ value: "warning" as const, label: "warnings and errors" }, { value: "error" as const, label: "errors only" }]} />
      </label>
      <label>Engine
        <input class="field" placeholder="Hub's default" value={vakten.prefs.engine ?? ""} onchange={(e) => setVakten({ engine: e.currentTarget.value || undefined })} />
      </label>
    </div>
    <div class="row">
      <span class="say">
        {vakten.state === "guarded" ? `Guarded from ${vakten.guarded_by}.` : vakten.state === "off" ? "Off." : vakten.state === "paused" ? "Paused: the panic button or a plan limit holds it." : vakten.state === "working" ? "Working now." : vakten.state === "waiting" ? "Waiting on you in its thread." : "Idle."}
        {vakten.unstamped} in the queue on every machine · {vakten.fixing} fixing now · {vakten.fix_threads_today} fix threads today.
      </span>
      <button class="btn" disabled={!vakten.prefs.on} onclick={look}>Look now</button>
      {#if vakten.thread}<button class="btn" onclick={() => hub.open(vakten!.thread!)}>Open Guard's chat</button>{/if}
    </div>
    {#if vakten.guarded_by}
      <div class="row"><span class="say">Guard on {vakten.guarded_by} reads this machine's problems, so this one stays still.</span></div>
    {/if}
    {#if hub.problemNews.length}
      <div class="label">What it told you</div>
      {#each hub.problemNews.slice(0, 6) as n, i (i)}<div class="news">{ago(n.ts)} · {n.text}</div>{/each}
    {/if}
  {:else if which === "rules"}
    <p class="say">
      Rules group, stamp or archive what comes in. These apply everywhere; a project can keep its own in <code>.hub/problems.toml</code>.
      Each <code>[[rule]]</code> matches on <code>match</code> (words, * for anything), <code>path</code>, <code>project</code>,
      <code>source</code>, <code>machine</code>, <code>level</code>, and sets <code>group</code>, <code>prio</code>, <code>lane</code>,
      <code>category</code> or <code>archive</code> ("worse" or "forever").
    </p>
    <textarea class="field toml" spellcheck="false" bind:value={text} placeholder={'[[rule]]\nname = "ssh hiccups"\nmatch = "could not reach * over ssh"\ngroup = "ssh"\nprio = "P3"\nlane = "quiet"'}></textarea>
    <div class="row"><button class="btn" disabled={text === rules?.global} onclick={saveRules}>Save rules</button></div>
    {#if rules?.files.length}
      <div class="label">Projects' own</div>
      {#each rules.files as f (f.path)}<div class="news">{f.project} · {f.rules} rules · <code>{f.path}</code></div>{/each}
    {/if}
    {#if rules?.suggestions.length}
      <div class="label">Suggested from what you did</div>
      {#each rules.suggestions as s, i (i)}
        <div class="row"><span class="grow">{s.text} <small>({s.why})</small></span><button class="btn" onclick={() => addRule(i)}>Make it a rule</button></div>
      {/each}
    {/if}
  {:else if which === "connect"}
    <p class="say">
      Any program with a Sentry SDK can report here: put its DSN in <code>sentry_sdk.init(dsn="…")</code> or <code>SENTRY_DSN</code>.
      Cron jobs that send check-ins show up below. A program with OpenTelemetry sends JSON
      (<code>OTEL_EXPORTER_OTLP_PROTOCOL=http/json</code>) to its OTLP address, with <code>Authorization: Bearer &lt;key&gt;</code>.
    </p>
    {#each sources as s (s.id)}
      <div class="source">
        <span class="sname">{s.name}<small>{s.last_heard ? `heard ${ago(s.last_heard)}` : "not heard yet"}</small></span>
        <code class="dsn" title={s.dsn}>{s.dsn}</code>
        <button class="icon" data-tip="Copy the DSN" onclick={() => copy(s.dsn)}><Icon name="page" size={13} /></button>
        <button class="icon" data-tip="Copy the OpenTelemetry address and key" onclick={() => copy(`${s.otlp}  (Authorization: Bearer ${s.key})`)}><Icon name="chart" size={13} /></button>
        <Select value={s.quiet_after_hours ?? 0} onchange={(h) => quietAfter(s, h)} tip="A problem when it sends nothing for this long"
          options={[0, 1, 6, 24, 72, 168].map((h) => ({ value: h, label: !h ? "never quiet" : `quiet after ${h < 24 ? `${h} h` : `${h / 24} d`}` }))} />
        <button class="icon" data-tip="Remove: the program can no longer report" onclick={() => removeSource(s)}><Icon name="x" size={13} /></button>
      </div>
    {/each}
    <form class="add" onsubmit={(e) => (e.preventDefault(), addSource())}>
      <input class="field" placeholder="Program, e.g. Aftermath" bind:value={newName} />
      <Select bind:value={newFolder} tip="The folder threads on its problems work in"
        options={[{ value: "", label: "Folder (optional)" }, ...hub.projects.map((p) => ({ value: p.path, label: p.name }))]} />
      <button class="btn" disabled={!newName.trim()}>Add</button>
    </form>
    {#if monitors.length}
      <div class="label">Cron jobs</div>
      {#each monitors as m (m.source + m.slug)}
        <div class="news"><span class="st {m.status}">{m.status}</span> {m.source} / {m.slug} · {m.last_ok ? `last ok ${ago(m.last_ok)}` : "never ok"}{m.every_ms ? ` · every ${Math.round(m.every_ms / 60000)} min` : ""}</div>
      {/each}
    {/if}
  {/if}
</section>

<style>
  .panel { margin-top: 14px; padding: 12px 14px; background: var(--surface); border-radius: var(--r-md); max-width: 96ch; font-size: 13px; }
  .say { color: var(--text-3); margin: 0 0 8px; max-width: var(--prose); }
  .label { color: var(--muted); font-size: 12px; margin: 12px 0 6px; }
  code { font-family: var(--mono); font-size: 12px; }
  .grid { display: flex; flex-wrap: wrap; gap: 14px; align-items: center; color: var(--text-3); }
  .grid label { display: inline-flex; align-items: center; gap: 6px; }
  .row { display: flex; align-items: center; gap: 8px; margin-top: 8px; }
  .grow { flex: 1; color: var(--text-2); }
  small { color: var(--muted); }
  .news { color: var(--text-3); margin: 3px 0; }
  .btn { height: 26px; padding: 0 10px; font-size: 13px; }
  .icon { display: grid; place-items: center; width: 26px; height: 26px; border-radius: var(--r-sm); color: var(--muted); flex: none; }
  .icon:hover { color: var(--text); background: var(--surface-hover); }
  .field { height: 26px; padding: 0 8px; background: var(--bg); border: 1px solid var(--line-strong); border-radius: var(--r-md); font-size: 13px; outline: none; color: var(--text-2); min-width: 0; }
  .field:focus { border-color: var(--accent); }
  .num { width: 7ch; }
  .toml { width: 100%; height: 180px; padding: 8px; font-family: var(--mono); font-size: 12px; resize: vertical; }
  .source { display: flex; align-items: center; gap: 8px; min-height: 32px; }
  .sname { color: var(--text); min-width: 14ch; display: flex; flex-direction: column; }
  .dsn { color: var(--text-3); min-width: 0; flex: 1; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .add { display: flex; gap: 8px; margin-top: 8px; }
  .add input { flex: 1; max-width: 30ch; }
  .st { font-size: 11px; padding: 0 4px; border-radius: var(--r-sm); border: 1px solid var(--line-strong); }
  .st.error, .st.missed { color: var(--red); border-color: var(--red); }
  .st.ok { color: var(--green); border-color: var(--green); }
</style>
