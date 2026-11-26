<!-- Recipes: jobs saved to run again by hand on any project, like "Update dependencies". Unlike a routine nothing runs by itself, and a run does the work: a thread that stops at an open pull request. Pick the project at the top and press Run. Agents hand recipes over too; those wait at the top until they are kept or dismissed.
     Used by App.svelte. -->
<script lang="ts">
  import Page from "./Page.svelte";
  import { resource } from "../resource";
  import { modal } from "../pop";
  // Recipes: jobs saved to run again by hand on any project, like "Update dependencies". Unlike a
  // routine nothing runs by itself, and a run does the work: a thread that stops at an open pull
  // request. Pick the project at the top and press Run. Agents hand recipes over too; those wait
  // at the top until they are kept or dismissed.
  import { onMount, tick } from "svelte";
  import { api } from "../api";
  import type { Recipe } from "../bindings/Recipe";
  import type { RecipeDraft } from "../bindings/RecipeDraft";
  import type { RecipeRun } from "../bindings/RecipeRun";
  import type { RecipeTemplate } from "../bindings/RecipeTemplate";
  import { ago, folderName, isChatThread, statusLabel } from "../format";
  import { engineOptions, engines, loadEngines } from "../providers.svelte";
  import { shortPath } from "../refs";
  import { hub, unread } from "../state.svelte";
  import Icon from "./Icon.svelte";
  import Quiet from "./Quiet.svelte";
  import Ref from "./Ref.svelte";
  import Select from "./Select.svelte";

  const recipes = resource("recipes", () => api.recipes(), { poll: 15_000 });
  const data = $derived($recipes.data ?? null);
  const failed = $derived($recipes.error ?? null);
  let busy = $state<string | null>(null);
  let open = $state<Record<string, boolean>>({});
  /** The note for the next run, per recipe. */
  let notes = $state<Record<string, string>>({});
  /** The recipe being written: a new one (no id), one being changed, or a suggestion being kept. */
  let editing = $state<(RecipeDraft & { id?: string; suggested?: boolean }) | null>(null);
  /** The project recipes run on. */
  let target = $state("");
  /** A delete waiting for its second click. */
  let armed = $state(false);
  let nameInput = $state<HTMLInputElement | null>(null);

  onMount(() => {
    loadEngines();
  });

  // Projects first, then other folders threads worked in, all on this machine.
  const folders = $derived.by(() => {
    const out: string[] = hub.projects.map((p) => p.path);
    for (const t of hub.threads) if (!t.host && !isChatThread(t) && !out.includes(t.cwd) && out.length < 40) out.push(t.cwd);
    return out;
  });
  $effect(() => {
    if (!target && folders.length) target = hub.project && folders.includes(hub.project) ? hub.project : folders[0];
  });

  const all = $derived(data?.recipes ?? []);
  const kept = $derived(all.filter((r) => !r.suggested));
  const suggested = $derived(all.filter((r) => r.suggested));
  const templates = $derived(data?.templates ?? []);
  /** Starters not kept yet. */
  const missing = $derived(templates.filter((t) => !kept.some((r) => r.template === t.id || r.name.toLowerCase() === t.name.toLowerCase())));
  const projectName = (path: string) => hub.projects.find((p) => p.path === path)?.name ?? folderName(path);
  const threadOf = (run: RecipeRun | undefined) => (run ? hub.threads.find((t) => t.id === run.thread) : undefined);
  /** Where a run of `r` goes: its own folder, or the picked project. */
  const whereTo = (r: Recipe) => r.folder ?? target;

  function lastLine(run: RecipeRun): string {
    const t = threadOf(run);
    const a = ago(run.at);
    const when = a === "now" ? "just now" : `${a} ago`;
    // A run that is idle again has finished; its thread says how.
    const state = !t ? "Ran" : t.status === "idle" ? "Finished" : statusLabel[t.status];
    return `${state} in ${projectName(run.folder)}, ${when}`;
  }

  async function act(key: string, f: () => Promise<unknown>) {
    busy = key;
    armed = false;
    try {
      await f();
      await recipes.refresh();
    } catch (e) {
      hub.fail(e);
    } finally {
      busy = null;
    }
  }

  function run(r: Recipe) {
    const folder = whereTo(r);
    if (!folder) return;
    act(r.id, async () => {
      await api.runRecipe(r.id, folder, notes[r.id]);
      notes[r.id] = "";
    });
  }

  const draftOf = (r: Recipe): RecipeDraft & { id: string; suggested?: boolean } => ({
    id: r.id,
    name: r.name,
    about: r.about,
    prompt: r.prompt,
    engine: r.engine ?? undefined,
    folder: r.folder ?? undefined,
    template: r.template ?? undefined,
    suggested: !!r.suggested,
  });

  async function edit(d: RecipeDraft & { id?: string; suggested?: boolean }) {
    editing = d;
    armed = false;
    await tick();
    nameInput?.focus();
  }

  function blank() {
    edit({ name: "", about: "", prompt: "" });
  }

  /** Fills the new recipe from a starter; everything can still be changed. */
  function startFrom(t: RecipeTemplate) {
    if (!editing) return;
    editing.name = t.name;
    editing.about = t.about;
    editing.prompt = t.prompt;
    editing.template = t.id;
  }

  function addTemplate(t: RecipeTemplate) {
    act(`t:${t.id}`, () => api.createRecipe({ name: t.name, about: t.about, prompt: t.prompt, template: t.id }));
  }

  const ready = $derived(!!editing && !!editing.name.trim() && !!editing.prompt.trim());

  function save(ev?: Event) {
    ev?.preventDefault();
    const e = editing;
    if (!e || !ready) return;
    const { id, suggested: _, ...draft } = e;
    act("save", async () => {
      await (id ? api.updateRecipe(id, draft) : api.createRecipe(draft));
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
      await api.deleteRecipe(id);
      editing = null;
    });
  }

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

<Page title="Recipes">
  {#snippet head()}
    {#if kept.length}<span class="label">{kept.length}</span>{/if}
    <button class="btn primary push" onclick={blank}><Icon name="plus" size={13} /> New recipe</button>
  {/snippet}
  <div class="wrap">
    <p class="summary">Jobs to run again.</p>

    {#if failed}<p class="say bad">{failed}</p>{/if}

    <div class="target">
      <label class="label" for="r-target">Run on</label>
      <Select id="r-target" bind:value={target} options={folders.map((f) => ({ value: f, label: projectName(f) }))} />
      {#if target}<span class="muted mono" title={target}>{shortPath(target)}</span>{/if}
    </div>

    {#if suggested.length}
      <section class="suggested">
        <span class="label">Suggested by agents</span>
        {#each suggested as r (r.id)}
          {@const s = r.suggested}
          <div class="suggestion">
            <div class="s-main">
              <span class="name">{r.name}</span>
              {#if r.about}<span class="about">{r.about}</span>{/if}
              <span class="meta">
                {#if s?.thread}
                  From <button class="inline" onclick={() => s.thread && hub.open(s.thread)}>{s.by}</button>
                {:else if s}
                  From {s.by}
                {/if}
                {#if s?.why}<span class="sep"> · </span>{s.why}{/if}
              </span>
            </div>
            <div class="acts">
              <button class="btn primary" disabled={busy === r.id} onclick={() => act(r.id, () => api.keepRecipe(r.id))}>Keep</button>
              <button class="btn" onclick={() => edit(draftOf(r))}>Read and change</button>
              <button class="btn" disabled={busy === r.id} onclick={() => act(r.id, () => api.deleteRecipe(r.id))}>Dismiss</button>
            </div>
          </div>
        {/each}
      </section>
    {/if}

    {#if kept.length}
      <div class="list">
        {#each kept as r (r.id)}
          {@const last = r.runs[0]}
          {@const t = threadOf(last)}
          {@const where = whereTo(r)}
          <div class="recipe" class:open={open[r.id]}>
            <div class="row">
              <button class="run-btn" title={where ? `Run on ${projectName(where)}` : "Pick a project first"} disabled={!where || busy === r.id} onclick={() => run(r)}>
                <Icon name="play" size={12} />Run
              </button>
              <button class="main" aria-expanded={!!open[r.id]} title={open[r.id] ? "Hide" : "Show the prompt, a note for this run and earlier runs"} onclick={() => (open[r.id] = !open[r.id])}>
                <span class="name">{r.name}<span class="chev"><Icon name="chevron" size={12} /></span></span>
                <span class="meta">
                  {#if r.about}<span class="about-line">{r.about}</span>{/if}
                  {#if r.folder}{#if r.about}<span class="sep">·</span>{/if}<Ref kind="project" label={`Only ${projectName(r.folder)}`} title={r.folder} />{/if}
                  {#if r.engine}<span class="sep">·</span><span>{r.engine}</span>{/if}
                </span>
              </button>
              <div class="last">
                {#if last}
                  <button class="report-link" title="Open the run's thread" onclick={() => hub.open(last.thread)}>
                    {#if t}<span class="dot {t.status}" class:unread={unread(t)}></span>{/if}{lastLine(last)}
                  </button>
                {:else}
                  <span class="none">Not run yet</span>
                {/if}
              </div>
              <div class="acts">
                <button class="icon" title="Change" aria-label="Change" onclick={() => edit(draftOf(r))}><Icon name="pen" size={13} /></button>
              </div>
            </div>
            {#if open[r.id]}
              <div class="body">
                <pre class="prompt">{r.prompt}</pre>
                <form class="note" onsubmit={(e) => { e.preventDefault(); run(r); }}>
                  <input bind:value={notes[r.id]} placeholder="Anything to add this time, like only the api crate" spellcheck="false" aria-label="Note for this run" />
                  <button class="btn" type="submit" disabled={!where || busy === r.id}><Icon name="play" size={11} /> Run on {where ? projectName(where) : "…"}</button>
                </form>
                {#if r.runs.length}
                  <div class="runs">
                    <span class="label">Runs</span>
                    {#each r.runs.slice(0, 8) as run (run.thread)}
                      {@const rt = threadOf(run)}
                      <button class="run" disabled={!rt} onclick={() => hub.open(run.thread)}>
                        <span class="dot {rt?.status ?? 'idle'}"></span>{new Date(run.at).toLocaleString([], { weekday: "short", day: "numeric", month: "short", hour: "2-digit", minute: "2-digit" })}
                        <span class="muted">{projectName(run.folder)}{rt ? ` · ${rt.status === "idle" ? "Finished" : statusLabel[rt.status]}` : " · thread gone"}</span>
                      </button>
                    {/each}
                  </div>
                {/if}
              </div>
            {/if}
          </div>
        {/each}
      </div>

      {#if missing.length}
        <div class="starters">
          <span class="label">Starters</span>
          {#each missing as t (t.id)}
            <button class="chip" title={t.about} disabled={busy === `t:${t.id}`} onclick={() => addTemplate(t)}><Icon name="plus" size={11} />{t.name}</button>
          {/each}
        </div>
      {/if}
    {:else if !data && !failed}
      <Quiet kind="loading" />
    {:else if data}
      <section class="empty">
        <Quiet text="No recipes" />
        <div class="templates">
          {#each templates as t (t.id)}
            <div class="template">
              <span class="t-name">{t.name}</span>
              <p>{t.about}</p>
              <button class="btn" disabled={busy === `t:${t.id}`} onclick={() => addTemplate(t)}><Icon name="plus" size={12} /> Keep</button>
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
    <div class="dialog" role="dialog" aria-modal="true" aria-label={e.id ? "Change recipe" : "New recipe"}><form onsubmit={save}>
      <div class="d-head">
        <span class="label">{e.suggested ? "Suggested recipe" : e.id ? "Change recipe" : "New recipe"}</span>
        <button type="button" class="icon" title="Close (Esc)" aria-label="Close" onclick={() => (editing = null)}><Icon name="x" size={13} /></button>
      </div>
      <input class="title" bind:this={nameInput} bind:value={e.name} placeholder="Name" spellcheck="false" aria-label="Name" />

      {#if !e.id && templates.length}
        <div class="from">
          <span class="label">Start from</span>
          {#each templates as t (t.id)}
            <button type="button" class="chip" class:on={e.template === t.id} title={t.about} onclick={() => startFrom(t)}>{t.name}</button>
          {/each}
        </div>
      {/if}

      <div class="grid">
        <label for="r-about">About</label>
        <input id="r-about" bind:value={e.about} placeholder="What it does" spellcheck="false" />
        <label for="r-folder">Runs on</label>
        <div class="line">
          <input id="r-folder" class="mono" bind:value={e.folder} list="r-folders" spellcheck="false" placeholder="Any project" title={e.folder ?? ""} />
          <datalist id="r-folders">{#each folders as f (f)}<option value={f}>{shortPath(f)}</option>{/each}</datalist>
        </div>
        <label for="r-engine">Engine</label>
        <Select id="r-engine" bind:value={e.engine} options={engineOptions(e.engine)} />
        <label for="r-prompt">What to do</label>
        <textarea id="r-prompt" bind:value={e.prompt} rows="9" placeholder="What to do"></textarea>
      </div>

      <div class="d-foot">
        {#if e.id && !e.suggested}
          <button type="button" class="btn danger" class:armed onclick={remove}>{armed ? "Delete for good?" : "Delete"}</button>
        {/if}
        <span class="hint" aria-hidden="true"></span>
        <button type="submit" class="btn primary" disabled={busy === "save" || !ready}>{e.suggested ? "Keep" : e.id ? "Save" : "Add recipe"}</button>
      </div>
    </form></div>
  </div>
{/if}

<style>
  /* The list keeps a width the eye can follow from Run to the last run; ch grows with the text size. */
  .push { margin-left: 8px; }
  .summary { color: var(--text-3); font-size: 13px; margin: 6px 0 0; max-width: var(--prose); }
  .label, .muted { color: var(--muted); font-size: 12px; font-weight: 400; }
  .mono { font-family: var(--mono); }
  .say { color: var(--text-3); font-size: 13px; margin: 6px 0; }
  .say.bad { color: var(--red); }
  .btn { min-height: 26px; padding: 0 10px; font-size: 13px; display: inline-flex; align-items: center; gap: 4px; }
  .btn.armed { background: var(--red-bg); color: var(--red); }
  .icon { display: grid; place-items: center; width: 28px; height: 28px; border-radius: var(--r-sm); color: var(--muted); }
  .icon:hover:not(:disabled) { color: var(--text); background: var(--surface-hover); }
  .inline { color: var(--accent); font-size: inherit; }
  .inline:hover { text-decoration: underline; }
  .sep { opacity: 0.7; margin: 0 2px; }

  .target { display: flex; flex-wrap: wrap; align-items: center; gap: 6px 10px; margin-top: 16px; }
  .target .muted { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; min-width: 0; max-width: 50ch; }

  .suggested { margin-top: 18px; display: flex; flex-direction: column; gap: 6px; }
  .suggestion { display: flex; flex-wrap: wrap; align-items: center; gap: 8px 16px; background: var(--surface); border-radius: var(--r-md); padding: 10px 12px; }
  .s-main { display: flex; flex-direction: column; gap: 2px; flex: 1 1 36ch; min-width: 0; }
  .about { color: var(--text-3); font-size: 13px; }
  .suggestion .meta { color: var(--muted); font-size: 12px; }
  .suggestion .acts { display: inline-flex; flex-wrap: wrap; gap: 6px; }

  .list { margin-top: 16px; border-top: 1px solid var(--line); container-type: inline-size; }
  .recipe { border-bottom: 1px solid var(--line); }
  .row { display: grid; grid-template-columns: auto minmax(0, 46ch) minmax(18ch, max-content) auto; justify-content: start; align-items: center; gap: 6px 14px; min-height: var(--row-min); padding: var(--row-pad) 4px var(--row-pad) 6px; border-radius: var(--r-sm); }
  .recipe:not(.open) .row:hover { background: var(--surface); }
  .run-btn { display: inline-flex; align-items: center; gap: 4px; min-height: 26px; padding: 0 10px; border-radius: 999px; border: 1px solid var(--line-strong); color: var(--text-2); font-size: 13px; white-space: nowrap; }
  .run-btn:hover:not(:disabled) { color: var(--text); border-color: var(--accent); }
  .run-btn:disabled { opacity: 0.45; cursor: default; }
  .main { display: flex; flex-direction: column; gap: var(--row-gap); min-width: 0; text-align: left; padding: 2px 0; }
  .name { color: var(--text); font-size: 14px; display: inline-flex; align-items: center; gap: 6px; min-width: 0; white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
  .chev { display: grid; color: var(--muted); opacity: 0; transition: transform 0.12s var(--ease), opacity 0.12s var(--ease); }
  .main:hover .chev, .main:focus-visible .chev, .open .chev { opacity: 1; }
  .open .chev { transform: rotate(90deg); }
  .row .meta { display: inline-flex; align-items: center; gap: 4px; min-width: 0; color: var(--muted); font-size: 12px; white-space: nowrap; overflow: hidden; }
  .about-line { overflow: hidden; text-overflow: ellipsis; min-width: 0; }
  .meta :global(.glyph) { width: 11px; height: 11px; margin-right: 4px; }
  .last { display: flex; align-items: center; min-width: 0; }
  .report-link { display: inline-flex; align-items: center; gap: 6px; color: var(--text-2); font-size: 13px; white-space: nowrap; min-width: 0; overflow: hidden; text-overflow: ellipsis; }
  .report-link:hover { color: var(--text); text-decoration: underline; text-underline-offset: 3px; text-decoration-color: var(--line-strong); }
  .none { color: var(--muted); font-size: 13px; white-space: nowrap; }
  .acts { display: inline-flex; align-items: center; gap: 2px; }
  /* Narrow: the last run moves under the name, Run and Change stay on the first line. */
  @container (max-width: 620px) {
    .row { grid-template-columns: auto minmax(0, 1fr) auto; justify-content: stretch; }
    .last { grid-column: 2; grid-row: 2; }
    .acts { grid-column: 3; grid-row: 1; }
  }

  .body { padding: 2px 4px 16px 6px; display: flex; flex-direction: column; gap: 10px; max-width: var(--prose); }
  .prompt { margin: 0; background: var(--surface); border-radius: var(--r-md); padding: 10px 14px; font-family: var(--mono); font-size: 12px; line-height: 1.5; color: var(--text-2); white-space: pre-wrap; overflow-wrap: anywhere; max-height: 300px; overflow-y: auto; }
  .note { display: flex; flex-wrap: wrap; gap: 8px; align-items: center; }
  .note input { flex: 1 1 30ch; }
  .runs { display: flex; flex-direction: column; }
  .runs .label { padding: 0 8px 2px; }
  .run { display: flex; align-items: center; gap: 8px; min-height: 26px; padding: 0 8px; border-radius: var(--r-sm); color: var(--text-2); font-size: 13px; text-align: left; }
  .run:hover:not(:disabled) { background: var(--surface); }

  .chip { display: inline-flex; align-items: center; gap: 4px; min-height: 24px; padding: 0 8px; border-radius: 999px; border: 1px solid var(--line-strong); color: var(--text-2); font-size: 12px; white-space: nowrap; }
  .chip:hover:not(:disabled) { background: var(--surface-hover); color: var(--text); }
  .chip:disabled { opacity: 0.5; cursor: default; }
  .chip.on { border-color: var(--accent); color: var(--text); }
  .starters { display: flex; flex-wrap: wrap; align-items: center; gap: 6px 8px; margin-top: 16px; }

  .empty { margin-top: 22px; }
  .templates { display: grid; grid-template-columns: repeat(auto-fill, minmax(min(100%, 28ch), 1fr)); gap: 10px; }
  .template { background: var(--surface); border-radius: var(--r-md); padding: 12px 14px; display: flex; flex-direction: column; align-items: flex-start; gap: 4px; }
  .t-name { color: var(--text); font-size: 14px; font-weight: 500; }
  .template p { color: var(--text-3); font-size: 13px; margin: 4px 0 8px; flex: 1; }

  input, textarea { min-height: 28px; padding: 0 8px; background: var(--bg); border: 1px solid var(--line-strong); border-radius: var(--r-md); color: var(--text); font-size: 13px; outline: none; min-width: 0; }
  textarea { padding: 8px 8px; resize: vertical; font-family: var(--mono); line-height: 1.5; }
  input:focus, textarea:focus { border-color: var(--accent); }

  .backdrop { position: fixed; inset: 0; background: rgba(0, 0, 0, 0.4); z-index: 50; display: flex; justify-content: center; align-items: flex-start; padding: 8vh 16px 16px; overflow-y: auto; }
  .dialog { font-size: 13px; width: min(62em, 100%); background: var(--surface-2); border: 1px solid var(--line-strong); border-radius: var(--r-lg); box-shadow: 0 24px 60px rgba(0, 0, 0, 0.5); padding: 12px 20px 16px; }
  .d-head { display: flex; align-items: center; justify-content: space-between; margin-right: -8px; }
  .dialog .title { width: 100%; background: none; border: 0; outline: none; font-size: 18px; color: var(--text); padding: 4px 0 10px; }
  .dialog .title::placeholder { color: var(--muted); }
  .from { display: flex; flex-wrap: wrap; align-items: center; gap: 6px; padding-bottom: 14px; margin-bottom: 14px; border-bottom: 1px solid var(--line); }
  .from .label { margin-right: 2px; }
  .grid { display: grid; grid-template-columns: max-content minmax(0, 1fr); gap: 8px 14px; align-items: center; }
  .grid label { color: var(--text-3); font-size: 13px; min-width: 7em; }
  .grid label[for="r-prompt"] { align-self: start; padding-top: 6px; }
  .line { display: flex; gap: 8px; align-items: center; min-width: 0; }
  .line input { flex: 1; }
  .note-text { color: var(--muted); font-size: 12px; margin: 10px 0 0; }
  .d-foot { display: flex; flex-wrap: wrap; align-items: center; gap: 10px; margin-top: 14px; }
  .d-foot .hint { margin-left: auto; }
</style>
