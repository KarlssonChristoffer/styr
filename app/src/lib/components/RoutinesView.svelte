<!-- Routines: work Hub starts by itself on a schedule, like a daily look through the code for bugs and weak spots. Each run is a thread that only reports; answering in it gets things fixed. The page is the list: what each routine last found and when it runs next, with the actions beside the name. Writing one happens in a dialog, which also offers the starter routines.
     Used by App.svelte. -->
<script lang="ts">
  import Page from "./Page.svelte";
  import { poll } from "../poll";
  import { modal } from "../pop";
  // Routines: work Hub starts by itself on a schedule, like a daily look through the code for bugs
  // and weak spots. Each run is a thread that only reports; answering in it gets things fixed.
  // The page is the list: what each routine last found and when it runs next, with the actions
  // beside the name. Writing one happens in a dialog, which also offers the starter routines.
  import { onMount, tick } from "svelte";
  import { api } from "../api";
  import type { Every } from "../bindings/Every";
  import type { Routine } from "../bindings/Routine";
  import type { RoutineDraft } from "../bindings/RoutineDraft";
  import type { RoutineRun } from "../bindings/RoutineRun";
  import type { Routines } from "../bindings/Routines";
  import type { RoutineTemplate } from "../bindings/RoutineTemplate";
  import type { Schedule } from "../bindings/Schedule";
  import { ago, folderName, isChatThread } from "../format";
  import { engineOptions, engines, loadEngines } from "../providers.svelte";
  import { shortPath } from "../refs";
  import { hub, unread } from "../state.svelte";
  import Icon from "./Icon.svelte";
  import Quiet from "./Quiet.svelte";
  import Markdown from "./Markdown.svelte";
  import Ref from "./Ref.svelte";
  import Switch from "./Switch.svelte";
  import Select from "./Select.svelte";

  let data = $state<Routines | null>(null);
  let failed = $state<string | null>(null);
  let busy = $state<string | null>(null);
  let open = $state<Record<string, boolean>>({});
  /** The routine being written: a new one (no id) or one being changed. */
  let editing = $state<(RoutineDraft & { id?: string }) | null>(null);
  /** The folder starter routines are added to. */
  let folder = $state("");
  /** A delete waiting for its second click. */
  let armed = $state(false);
  let nameInput = $state<HTMLInputElement | null>(null);

  const DAYS = ["Monday", "Tuesday", "Wednesday", "Thursday", "Friday", "Saturday", "Sunday"];
  const EVERY: [Every, string][] = [["day", "Every day"], ["weekdays", "Weekdays"], ["week", "Once a week"], ["hours", "Every few hours"]];

  async function load() {
    try {
      data = await api.routines();
      failed = null;
    } catch (e) {
      failed = e instanceof Error ? e.message : String(e);
    }
  }

  onMount(() => {
    load();
    loadEngines();
    const timer = poll(load, 15_000);
    return () => timer();
  });

  // Projects first, then other folders threads worked in.
  const folders = $derived.by(() => {
    const out: string[] = hub.projects.map((p) => p.path);
    for (const t of hub.threads) if (!t.host && !isChatThread(t) && !out.includes(t.cwd) && out.length < 40) out.push(t.cwd);
    return out;
  });
  const folderOptions = $derived(folders.map((f) => ({ value: f, label: projectName(f) })));
  $effect(() => {
    if (!folder && folders.length) folder = folders[0];
  });

  const routines = $derived(data?.routines ?? []);
  const templates = $derived(data?.templates ?? []);
  const projectName = (path: string) => hub.projects.find((p) => p.path === path)?.name ?? folderName(path);
  const inFolder = (t: RoutineTemplate, f: string) => routines.some((r) => r.template === t.id && r.folder === f);
  /** Starters not yet added to the chosen folder. */
  const missing = $derived(templates.filter((t) => !inFolder(t, folder)));

  function time(at: number): string {
    return `${String(Math.floor(at / 60)).padStart(2, "0")}:${String(at % 60).padStart(2, "0")}`;
  }

  function when(s: Schedule): string {
    switch (s.every) {
      case "day": return `Every day at ${time(s.at)}`;
      case "weekdays": return `Weekdays at ${time(s.at)}`;
      case "week": return `${DAYS[s.weekday]}s at ${time(s.at)}`;
      case "hours": return s.hours === 1 ? "Every hour" : `Every ${s.hours} hours`;
    }
  }

  function next(ms: number | null): string {
    if (ms == null) return "Paused";
    if (ms <= Date.now()) return "Starts within a minute";
    const d = new Date(ms);
    const today = new Date();
    const tomorrow = new Date(today.getTime() + 86_400_000);
    const day = d.toDateString() === today.toDateString() ? "today" : d.toDateString() === tomorrow.toDateString() ? "tomorrow" : DAYS[(d.getDay() + 6) % 7];
    return `Next ${day} ${time(d.getHours() * 60 + d.getMinutes())}`;
  }

  const stateLabel = (r: RoutineRun) => (r.state === "working" ? "Running" : r.state === "done" ? "Reported" : "Failed");

  /** The routine's ring, on the same scale as threads: working, failed, a report not read yet, idle. */
  function ring(r: Routine): string {
    const last = r.runs[0];
    if (last?.state === "working") return "running";
    if (last?.state === "failed") return "error";
    const t = last?.thread ? hub.threads.find((x) => x.id === last.thread) : undefined;
    return t && unread(t) ? "idle unread" : "idle";
  }

  function lastLine(r: Routine): string {
    const last = r.runs[0];
    if (!last) return "No run yet";
    const a = ago(last.state === "working" ? last.at : (last.ended_at ?? last.at));
    if (last.state === "working") return a === "now" ? "Running, just started" : `Running for ${a}`;
    return `${stateLabel(last)} ${a === "now" ? "just now" : `${a} ago`}`;
  }

  async function act(key: string, f: () => Promise<unknown>) {
    busy = key;
    armed = false;
    try {
      await f();
      await load();
    } catch (e) {
      hub.fail(e);
    } finally {
      busy = null;
    }
  }

  const draftOf = (r: Routine): RoutineDraft & { id: string } => ({
    id: r.id,
    name: r.name,
    folder: r.folder,
    engine: r.engine ?? undefined,
    prompt: r.prompt,
    schedule: { ...r.schedule },
    enabled: r.enabled,
    template: r.template ?? undefined,
  });

  async function edit(d: RoutineDraft & { id?: string }) {
    editing = d;
    armed = false;
    await tick();
    nameInput?.focus();
  }

  function blank() {
    edit({ name: "", folder: folder || folders[0] || "", prompt: "", schedule: { every: "day", at: 7 * 60, weekday: 0, hours: 6 }, enabled: true });
  }

  /** Fills the new routine from a starter; the name and prompt can still be changed. */
  function startFrom(t: RoutineTemplate) {
    if (!editing) return;
    editing.name = t.name;
    editing.prompt = t.prompt;
    editing.schedule = { ...t.schedule };
    editing.template = t.id;
  }

  function addTemplate(t: RoutineTemplate) {
    act(`t:${t.id}`, () => api.createRoutine({ name: t.name, folder, prompt: t.prompt, schedule: { ...t.schedule }, enabled: true, template: t.id }));
  }

  const ready = $derived(!!editing && !!editing.name.trim() && !!editing.prompt.trim() && !!editing.folder.trim());

  function save(ev?: Event) {
    ev?.preventDefault();
    const e = editing;
    if (!e || !ready) return;
    const { id, ...draft } = e;
    act("save", async () => {
      await (id ? api.updateRoutine(id, draft) : api.createRoutine(draft));
      editing = null;
    });
  }

  function remove() {
    const id = editing?.id;
    if (!id) return;
    if (!armed) {
      armed = true;
      return;
    }
    act("save", async () => {
      await api.deleteRoutine(id);
      editing = null;
    });
  }

  function toggle(r: Routine, on: boolean) {
    act(r.id, () => api.updateRoutine(r.id, { ...draftOf(r), enabled: on }));
  }

  function runNow(r: Routine) {
    act(r.id, async () => {
      const { thread } = await api.runRoutine(r.id);
      open[r.id] = true;
      return thread;
    });
  }

  const setTime = (s: Schedule, v: string) => {
    const [h, m] = v.split(":").map(Number);
    if (!Number.isNaN(h) && !Number.isNaN(m)) s.at = h * 60 + m;
  };

  function key(e: KeyboardEvent) {
    if (!editing) return;
    if (e.key === "Escape") {
      e.preventDefault();
      editing = null;
    } else if (e.key === "Enter" && (e.ctrlKey || e.metaKey)) {
      save(e);
    }
  }
</script>

<svelte:window onkeydown={key} />

<Page title="Routines">
  {#snippet head()}
    {#if routines.length}<span class="label">{routines.filter((r) => r.enabled).length} of {routines.length} on</span>{/if}
    <button class="btn primary push" onclick={blank}><Icon name="plus" size={13} /> New routine</button>
  {/snippet}
  <div class="wrap">
    <p class="summary">Runs by itself, reports in a thread.</p>

    {#if failed}<p class="say bad">{failed}</p>{/if}

    {#if routines.length}
      <div class="list">
        {#each routines as r (r.id)}
          {@const last = r.runs[0]}
          <div class="routine" class:off={!r.enabled} class:open={open[r.id]}>
            <div class="row">
              <span class="dot {ring(r)}" class:paused={!r.enabled} data-tip={last ? stateLabel(last) : "No run yet"}></span>
              <button class="main" aria-expanded={!!open[r.id]} data-tip={open[r.id] ? "Hide the last report" : "Show the last report"} onclick={() => (open[r.id] = !open[r.id])}>
                <span class="name">{r.name}<span class="chev"><Icon name="chevron" size={12} /></span></span>
                <span class="meta">
                  <Ref kind="project" label={projectName(r.folder)} title={r.folder} />
                  <span class="sep">·</span>
                  <span class="sched"><Icon name="clock" size={11} />{when(r.schedule)}</span>
                  {#if r.engine}<span class="sep">·</span><span>{r.engine}</span>{/if}
                </span>
              </button>
              <div class="last">
                {#if last?.thread}
                  <button class="report-link" data-tip="Open the run's thread" onclick={() => last.thread && hub.open(last.thread)}>{lastLine(r)}<Icon name="external" size={11} /></button>
                {:else}
                  <span class="none">{lastLine(r)}</span>
                {/if}
                <span class="next">{next(r.next_at)}</span>
              </div>
              <div class="acts">
                <button class="icon" data-tip="Run now" aria-label="Run now" disabled={busy === r.id || last?.state === "working"} onclick={() => runNow(r)}><Icon name="play" size={13} /></button>
                <button class="icon" data-tip="Change" aria-label="Change" onclick={() => edit(draftOf(r))}><Icon name="pen" size={13} /></button>
                <Switch on={r.enabled} label={r.enabled ? "On: runs on schedule" : "Paused"} disabled={busy === r.id} onchange={(on) => toggle(r, on)} />
              </div>
            </div>
            {#if open[r.id]}
              <div class="body">
                {#if !last}
                  <p class="say">No run. {r.enabled ? next(r.next_at) + "." : "Paused."} <button class="inline" onclick={() => runNow(r)}>Run it now</button></p>
                {:else if last.state === "working"}
                  <p class="say">Running since {ago(last.at)}. <button class="inline" onclick={() => last.thread && hub.open(last.thread)}>Watch it</button></p>
                {:else if last.report}
                  <div class="report" class:failed={last.state === "failed"}><Markdown text={last.report} rich /></div>
                  {#if last.thread}<p class="say"><button class="inline" onclick={() => last.thread && hub.open(last.thread)}>Open the thread</button></p>{/if}
                {:else}
                  <p class="say">The run ended without a report.</p>
                {/if}
                {#if r.runs.length > 1}
                  <div class="runs">
                    <span class="label">Earlier runs</span>
                    {#each r.runs.slice(1, 8) as run (run.at)}
                      <button class="run" disabled={!run.thread} onclick={() => run.thread && hub.open(run.thread)}>
                        <span class="dot {run.state === 'working' ? 'running' : run.state === 'failed' ? 'error' : 'idle'}"></span>{new Date(run.at).toLocaleString([], { weekday: "short", day: "numeric", month: "short", hour: "2-digit", minute: "2-digit" })}
                        <span class="muted">{stateLabel(run)}{run.by === "you" ? " · started by you" : ""}</span>
                      </button>
                    {/each}
                  </div>
                {/if}
              </div>
            {/if}
          </div>
        {/each}
      </div>

      {#if templates.length}
        <div class="starters">
          <span class="label">Starters for</span>
          <Select bind:value={folder} label="Folder" options={folderOptions} />
          {#each missing as t (t.id)}
            <button class="chip" data-tip={t.about} disabled={!folder || busy === `t:${t.id}`} onclick={() => addTemplate(t)}><Icon name="plus" size={11} />{t.name}</button>
          {:else}
            <span class="muted">All starters are in {projectName(folder)}.</span>
          {/each}
        </div>
      {/if}
    {:else if !data && !failed}
      <Quiet kind="loading" />
    {:else if data}
      <section class="empty">
        <Quiet text="No routines" />
        <div class="add">
          <span class="label">Add to</span>
          <Select bind:value={folder} label="Folder" options={folderOptions} />
        </div>
        <div class="templates">
          {#each templates as t (t.id)}
            <div class="template">
              <span class="t-name">{t.name}</span>
              <span class="sched"><Icon name="clock" size={11} />{when(t.schedule)}</span>
              <p>{t.about}</p>
              <button class="btn" disabled={!folder || busy === `t:${t.id}`} onclick={() => addTemplate(t)}><Icon name="plus" size={12} /> Add</button>
            </div>
          {/each}
        </div>
      </section>
    {/if}
  </div>
</Page>

{#if editing}
  {@const e = editing}
  <div class="backdrop" role="presentation" in:modal|global out:modal|global={{ out: true }} onclick={(ev) => ev.target === ev.currentTarget && (editing = null)}>
    <div class="dialog" role="dialog" aria-modal="true" aria-label={e.id ? "Change routine" : "New routine"}><form onsubmit={save}>
      <div class="d-head">
        <span class="label">{e.id ? "Change routine" : "New routine"}</span>
        <button type="button" class="icon" data-tip="Close (Esc)" aria-label="Close" onclick={() => (editing = null)}><Icon name="x" size={13} /></button>
      </div>
      <input class="title" bind:this={nameInput} bind:value={e.name} placeholder="Name it, like Nightly security look" spellcheck="false" aria-label="Name" />

      {#if !e.id && templates.length}
        <div class="from">
          <span class="label">Start from</span>
          {#each templates as t (t.id)}
            <button type="button" class="chip" class:on={e.template === t.id} data-tip={t.about} onclick={() => startFrom(t)}>{t.name}</button>
          {/each}
        </div>
      {/if}

      <div class="grid">
        <label for="r-folder">Folder</label>
        <div class="line">
          <input id="r-folder" class="mono" bind:value={e.folder} list="r-folders" spellcheck="false" data-tip={e.folder} />
          <datalist id="r-folders">{#each folders as f (f)}<option value={f}>{shortPath(f)}</option>{/each}</datalist>
        </div>
        <label for="r-every">When</label>
        <div class="line">
          <Select id="r-every" bind:value={e.schedule.every} options={EVERY.map(([value, label]) => ({ value, label }))} />
          {#if e.schedule.every === "week"}
            <Select bind:value={e.schedule.weekday} label="Day" options={DAYS.map((label, value) => ({ value, label }))} />
          {/if}
          {#if e.schedule.every === "hours"}
            <input class="num" type="number" min="1" max="168" bind:value={e.schedule.hours} aria-label="Hours" /> <span class="muted">hours</span>
          {:else}
            <input class="at" type="time" value={time(e.schedule.at)} onchange={(ev) => setTime(e.schedule, ev.currentTarget.value)} aria-label="Time" />
          {/if}
        </div>
        <label for="r-engine">Engine</label>
        <Select id="r-engine" bind:value={e.engine} options={engineOptions(e.engine)} />
        <label for="r-prompt">What to do</label>
        <textarea id="r-prompt" bind:value={e.prompt} rows="8" placeholder="Look through … and report …"></textarea>
      </div>

      <div class="d-foot">
        {#if e.id}
          <button type="button" class="btn danger" class:armed onclick={remove}>{armed ? "Delete for good?" : "Delete"}</button>
        {/if}
        <span class="muted hint">Ctrl+Enter saves · Esc closes</span>
        <button type="submit" class="btn primary" disabled={busy === "save" || !ready}>{e.id ? "Save" : "Add routine"}</button>
      </div>
    </form></div>
  </div>
{/if}

<style>
  /* The list keeps a width the eye can follow from name to actions; ch grows with the text size. */
  .push { margin-left: 8px; }
  .summary { color: var(--text-3); font-size: 13px; margin: 6px 0 0; max-width: var(--prose); }
  .label, .muted { color: var(--muted); font-size: 12px; font-weight: 400; }
  .say { color: var(--text-3); font-size: 13px; margin: 6px 0; }
  .say.bad { color: var(--red); }
  .note { color: var(--muted); font-size: 12px; margin: 10px 0 0; }
  .btn { height: 26px; padding: 0 10px; font-size: 13px; display: inline-flex; align-items: center; gap: 4px; }
  .btn.armed { background: var(--red-bg); color: var(--red); }
  .icon { display: grid; place-items: center; width: 28px; height: 28px; border-radius: var(--r-sm); color: var(--muted); }
  .icon:hover:not(:disabled) { color: var(--text); background: var(--surface-hover); }
  .icon:disabled { opacity: 0.4; cursor: default; }
  .inline { color: var(--accent); font-size: 13px; }
  .inline:hover { text-decoration: underline; }
  .sched { display: inline-flex; align-items: center; gap: 4px; }
  .sep { opacity: 0.7; }

  .list { margin-top: 20px; border-top: 1px solid var(--line); container-type: inline-size; }
  /* Columns sized to their content, so the last run and the actions sit right after the name. */
  .routine { border-bottom: 1px solid var(--line); }
  .row { display: grid; grid-template-columns: auto minmax(0, 46ch) minmax(18ch, max-content) auto; justify-content: start; align-items: center; gap: 6px 14px; min-height: var(--row-min); padding: var(--row-pad) 4px var(--row-pad) 6px; border-radius: var(--r-sm); }
  .routine:not(.open) .row:hover { background: var(--surface); }
  .dot.paused { opacity: 0.45; }
  .main { display: flex; flex-direction: column; gap: var(--row-gap); min-width: 0; text-align: left; padding: 2px 0; }
  .name { color: var(--text); font-size: 14px; display: inline-flex; align-items: center; gap: 6px; min-width: 0; white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
  .off .name { color: var(--text-3); }
  .chev { display: grid; color: var(--muted); opacity: 0; transition: transform 0.12s var(--ease), opacity 0.12s var(--ease); }
  .main:hover .chev, .main:focus-visible .chev, .open .chev { opacity: 1; }
  .open .chev { transform: rotate(90deg); }
  .meta { display: inline-flex; align-items: center; gap: 4px; min-width: 0; color: var(--muted); font-size: 12px; white-space: nowrap; overflow: hidden; }
  .meta :global(.glyph) { width: 11px; height: 11px; margin-right: 4px; }
  .last { display: flex; flex-direction: column; align-items: flex-start; gap: 2px; min-width: 0; }
  .report-link { display: inline-flex; align-items: center; gap: 4px; color: var(--text-2); font-size: 13px; white-space: nowrap; }
  .report-link :global(svg) { color: var(--muted); }
  .report-link:hover { color: var(--text); text-decoration: underline; text-underline-offset: 3px; text-decoration-color: var(--line-strong); }
  .none { color: var(--muted); font-size: 13px; white-space: nowrap; }
  .next { color: var(--muted); font-size: 12px; white-space: nowrap; }
  .acts { display: inline-flex; align-items: center; gap: 2px; }
  .acts :global(.switch) { margin-left: 8px; }
  /* Narrow: the last run moves under the name, the actions stay on the first line. */
  @container (max-width: 620px) {
    .row { grid-template-columns: auto minmax(0, 1fr) auto; justify-content: stretch; }
    .last { grid-column: 2; grid-row: 2; flex-direction: row; gap: 10px; }
    .acts { grid-column: 3; grid-row: 1; }
  }

  .body { padding: 2px 4px 16px 26px; }
  .report { background: var(--surface); border-radius: var(--r-md); padding: 10px 14px; font-size: 13px; max-width: var(--prose); max-height: 460px; overflow-y: auto; }
  .report.failed { color: var(--red); }
  .runs { display: flex; flex-direction: column; margin-top: 10px; max-width: var(--prose); }
  .runs .label { padding: 0 8px 2px; }
  .run { display: flex; align-items: center; gap: 8px; min-height: 26px; padding: 0 8px; border-radius: var(--r-sm); color: var(--text-2); font-size: 13px; text-align: left; }
  .run:hover:not(:disabled) { background: var(--surface); }

  .chip { display: inline-flex; align-items: center; gap: 4px; height: 24px; padding: 0 8px; border-radius: 999px; border: 1px solid var(--line-strong); color: var(--text-2); font-size: 12px; white-space: nowrap; }
  .chip:hover:not(:disabled) { background: var(--surface-hover); color: var(--text); }
  .chip:disabled { opacity: 0.5; cursor: default; }
  .chip.on { border-color: var(--accent); color: var(--text); }
  .starters { display: flex; flex-wrap: wrap; align-items: center; gap: 6px 8px; margin-top: 16px; }

  .empty { margin-top: 22px; }
  .add { display: flex; align-items: center; gap: 8px; margin-bottom: 12px; }
  .templates { display: grid; grid-template-columns: repeat(auto-fill, minmax(min(100%, 28ch), 1fr)); gap: 10px; }
  .template { background: var(--surface); border-radius: var(--r-md); padding: 12px 14px; display: flex; flex-direction: column; align-items: flex-start; gap: 4px; }
  .t-name { color: var(--text); font-size: 14px; font-weight: 500; }
  .template .sched { color: var(--muted); font-size: 12px; }
  .template p { color: var(--text-3); font-size: 13px; margin: 4px 0 8px; flex: 1; }

  input, textarea { height: 28px; padding: 0 8px; background: var(--bg); border: 1px solid var(--line-strong); border-radius: var(--r-md); color: var(--text); font-size: 13px; outline: none; min-width: 0; }
  textarea { height: auto; padding: 8px 8px; resize: vertical; font-family: var(--mono); line-height: 1.5; }
  input:focus, textarea:focus { border-color: var(--accent); }

  .backdrop { position: fixed; inset: 0; background: rgba(0, 0, 0, 0.4); z-index: 50; display: flex; justify-content: center; align-items: flex-start; padding: 8vh 16px 16px; overflow-y: auto; }
  .dialog { font-size: 13px; width: min(62em, 100%); background: var(--surface-2); border: 1px solid var(--line-strong); border-radius: var(--r-lg); box-shadow: 0 24px 60px rgba(0, 0, 0, 0.5); padding: 12px 20px 16px; }
  .d-head { display: flex; align-items: center; justify-content: space-between; margin-right: -8px; }
  .dialog .title { width: 100%; height: auto; background: none; border: 0; outline: none; font-size: 18px; color: var(--text); padding: 4px 0 10px; }
  .dialog .title::placeholder { color: var(--muted); }
  .from { display: flex; flex-wrap: wrap; align-items: center; gap: 6px; padding-bottom: 14px; margin-bottom: 14px; border-bottom: 1px solid var(--line); }
  .from .label { margin-right: 2px; }
  .grid { display: grid; grid-template-columns: max-content minmax(0, 1fr); gap: 8px 14px; align-items: center; }
  .grid label { color: var(--text-3); font-size: 13px; min-width: 7em; }
  .grid label[for="r-prompt"] { align-self: start; padding-top: 6px; }
  .line { display: flex; gap: 8px; align-items: center; min-width: 0; }
  .line input#r-folder { flex: 1; }
  .num { width: 6em; }
  .at { width: 8em; }
  .mono { font-family: var(--mono); }
  .d-foot { display: flex; align-items: center; gap: 10px; margin-top: 14px; }
  .d-foot .hint { margin-left: auto; }
</style>
