<!-- The project's focus from docs/FOCUS.md: where it is going, the stage it is in with what counts as delivered and what is left to test by hand, and the ideas parked for later. It sits on top of the overview so it is the first thing seen, and agents keep it current as they work.
     Used by ProjectView.svelte. -->
<script lang="ts">
  import { api } from "../api";
  import type { Check } from "../bindings/Check";
  import type { Focus } from "../bindings/Focus";
  import type { FocusEdit } from "../bindings/FocusEdit";
  import type { Stage } from "../bindings/Stage";
  import { prefs } from "../prefs.svelte";
  import { hub } from "../state.svelte";
  import Icon from "./Icon.svelte";

  // The project's focus from docs/FOCUS.md: where it is going, the stage it is in with what counts
  // as delivered and what is left to test by hand, and the ideas parked for later. It sits on top
  // of the overview so it is the first thing seen, and agents keep it current as they work.
  let {
    path,
    host = null,
    focus,
    onfocus,
    onstart,
    onfill,
  }: {
    path: string;
    host?: string | null;
    focus: Focus;
    onfocus: (f: Focus) => void;
    onstart: (text: string) => void;
    /** Puts text in the project's composer to finish before sending. */
    onfill: (text: string) => void;
  } = $props();

  // The stage on show: the active one unless another was picked on the road.
  let picked = $state<number | null>(null);
  $effect(() => {
    void path;
    picked = null;
  });
  const shown = $derived(picked ?? focus.active ?? Math.max(0, focus.stages.length - 1));
  const stage = $derived<Stage | null>(focus.stages[shown] ?? null);
  const isActive = $derived(focus.active === shown);
  const delivered = (s: Stage) => s.done.length > 0 && s.done.every((c) => c.done);
  const count = (s: Stage) => `${s.done.filter((c) => c.done).length}/${s.done.length}`;

  // What is already ticked folds away, so what is left stands out.
  let showDelivered = $state(false);
  const deliveredCount = $derived(stage?.done.filter((c) => c.done).length ?? 0);
  let showTested = $state(false);
  const openTests = $derived(stage?.test.filter((c) => !c.done) ?? []);
  const tested = $derived(stage?.test.filter((c) => c.done) ?? []);

  async function change(edit: Omit<FocusEdit, "path">) {
    try {
      onfocus(await api.editFocus({ path, ...edit }, host));
      return true;
    } catch (e) {
      hub.fail(e);
      return false;
    }
  }
  const tick = (c: Check) => change({ op: "tick", line: c.line, done: !c.done });

  // One input per list, keyed by where it adds: "done:<line>", "test:<line>" or "park".
  let drafts = $state<Record<string, string>>({});
  async function add(e: SubmitEvent, key: string, edit: Omit<FocusEdit, "path" | "text">) {
    e.preventDefault();
    const text = drafts[key]?.trim();
    if (text && (await change({ ...edit, text }))) drafts[key] = "";
  }

  function startItem(s: Stage, c: Check) {
    onstart(
      `${c.text}\n\nThis is part of "${s.title}", the active stage in docs/FOCUS.md. When it is done, tick it off there and add what I should test by hand under "To test".`,
    );
  }
  function reportProblem(s: Stage, c: Check) {
    onfill(`I tested "${c.text}" from "${s.title}" in docs/FOCUS.md and it does not work. What happened: `);
  }
  async function draft() {
    if (!focus.rule) await change({ op: "rule" });
    try {
      onstart((await api.focusDraft()).message);
    } catch (e) {
      hub.fail(e);
    }
  }
  // Focus mode for this project: only it shows in the lists until you leave.
  const remote = $derived(host && host !== hub.localHost ? host : null);
  const inFocus = $derived(prefs.focusOn?.cwd === path && prefs.focusOn.host === remote);
  function openFile() {
    hub.projectDoc = "docs/FOCUS.md";
    hub.projectTab = "docs";
  }
</script>

{#snippet checks(items: Check[], s: Stage | null)}
  {#each items as c (c.line)}
    <div class="check" class:done={c.done}>
      <button class="box" class:checked={c.done} onclick={() => tick(c)} data-tip={c.done ? "Mark as not done" : "Mark as done"}>
        {#if c.done}<Icon name="check" size={12} />{/if}
      </button>
      <span class="text">{c.text}</span>
      {#if s && !c.done}
        <button class="btn hover" onclick={() => startItem(s, c)} data-tip="Start a thread for this">Start</button>
      {/if}
    </div>
  {/each}
{/snippet}

{#snippet mode()}
  {#if inFocus}
    <button class="btn small on" onclick={() => hub.leaveFocus()} data-tip="Show every project again">In focus · Leave</button>
  {:else}
    <button class="btn small" onclick={() => hub.goIn(path, host)} data-tip="Only this project, with its desk">Go in</button>
    <button class="btn small" onclick={() => hub.enterFocus(path, host)} data-tip="Hide other projects until you leave">Focus on this</button>
  {/if}
{/snippet}

{#snippet adder(key: string, placeholder: string, edit: Omit<FocusEdit, "path" | "text">)}
  <form class="add" onsubmit={(e) => add(e, key, edit)}>
    <Icon name="plus" size={13} />
    <input bind:value={drafts[key]} {placeholder} spellcheck="false" />
  </form>
{/snippet}

<section class="focus">
  {#if !focus.exists}
    <div class="card-head"><h2>Focus</h2><span class="grow"></span>{@render mode()}</div>
    <p class="none">
      Where the project is going, in <code>docs/FOCUS.md</code>.
    </p>
    <div class="row">
      <button class="btn primary" onclick={draft} data-tip="A thread drafts it">Draft it with a thread</button>
      <button class="btn" onclick={() => change({ op: "start" })}>Start from a blank page</button>
    </div>
  {:else}
    <div class="card-head">
      <h2>Focus</h2>
      {#if focus.direction}<span class="direction" data-tip={focus.direction.about}>{focus.direction.title}</span>{/if}
      <button class="more" onclick={openFile} data-tip="Open docs/FOCUS.md">Edit file</button>
      {@render mode()}
    </div>

    {#if focus.stages.length}
      <div class="road">
        {#each focus.stages as s, i (s.line)}
          {#if i}<span class="leg" class:passed={delivered(focus.stages[i - 1])}></span>{/if}
          <button
            class="stop"
            class:on={i === shown}
            class:active={i === focus.active}
            class:passed={delivered(s) && i !== focus.active}
            onclick={() => (picked = i === focus.active ? null : i)}
            data-tip={s.about || s.title}
          >
            <span class="mark">{#if delivered(s) && i !== focus.active}<Icon name="check" size={10} />{/if}</span>
            <span class="name">{s.title}</span>
            {#if s.done.length}<span class="n">{count(s)}</span>{/if}
          </button>
        {/each}
      </div>
    {/if}

    <div class="body">
      <div class="stage">
        {#if stage}
          <div class="stage-head">
            <span class="label">Stage {shown + 1}</span>
            {#if isActive}<span class="now">Now</span>
            {:else}<button class="btn small" onclick={() => change({ op: "activate", line: stage.line }).then(() => (picked = null))}>Make this the stage now</button>{/if}
          </div>
          <h3>{stage.title}</h3>
          {#if stage.about}<p class="about">{stage.about}</p>{/if}

          {#if isActive && delivered(stage)}
            <div class="arrived">
              <Icon name="check" size={13} />
              <span>Everything here is delivered.{openTests.length ? ` Test the ${openTests.length} below, then move on.` : ""}</span>
              {#if focus.stages[shown + 1]}
                <button class="btn small" onclick={() => change({ op: "activate", line: focus.stages[shown + 1].line })}>On to {focus.stages[shown + 1].title}</button>
              {/if}
            </div>
          {/if}

          <div class="list-head">
            <span>Delivered when</span>
            {#if stage.done.length}<span class="n">{count(stage)}</span>{/if}
            {#if deliveredCount && deliveredCount < stage.done.length}<button class="more" onclick={() => (showDelivered = !showDelivered)}>{showDelivered ? "Hide delivered" : `Show ${deliveredCount} delivered`}</button>{/if}
          </div>
          {#if stage.done.length}<div class="progress"><span style:width="{(stage.done.filter((c) => c.done).length / stage.done.length) * 100}%"></span></div>{/if}
          {@render checks(showDelivered || deliveredCount === stage.done.length ? stage.done : stage.done.filter((c) => !c.done), isActive ? stage : null)}
          {@render adder(`done:${stage.line}`, "Add what counts as delivered", { op: "add", line: stage.line, list: "done" })}

          <div class="list-head">
            <span>To test</span>
            {#if openTests.length}<span class="n">{openTests.length} left</span>{/if}
            {#if tested.length}<button class="more" onclick={() => (showTested = !showTested)}>{showTested ? "Hide tested" : `${tested.length} tested`}</button>{/if}
          </div>
          {#each openTests as c (c.line)}
            <div class="check">
              <button class="box" onclick={() => tick(c)} aria-label="It works" data-tip="It works"></button>
              <span class="text">{c.text}</span>
              <button class="btn hover" onclick={() => reportProblem(stage, c)} data-tip="Start a thread about what went wrong">It doesn't work</button>
            </div>
          {:else}
            <p class="none small">{tested.length ? "All tested." : "Nothing to test."}</p>
          {/each}
          {#if showTested}{@render checks(tested, null)}{/if}
          {@render adder(`test:${stage.line}`, "Add something to test", { op: "add", line: stage.line, list: "test" })}

          {#if stage.ideas.length}
            <div class="list-head"><span>Ideas for this stage</span></div>
            <ul class="ideas">{#each stage.ideas as idea (idea.line)}<li>{idea.text}</li>{/each}</ul>
          {/if}
        {:else}
          <p class="none">No stages yet. Add them as <code>## Stage 1: …</code> headings in the file.</p>
        {/if}
      </div>

      <div class="side">
        {#if focus.direction}
          <div class="list-head"><span>Direction</span>{#if focus.direction.done.length}<span class="n">{count(focus.direction)}</span>{/if}</div>
          <p class="dir-title">{focus.direction.title}</p>
          {#if focus.direction.about}<p class="about">{focus.direction.about}</p>{/if}
          {#if focus.direction.done.length}
            <div class="sub">Done when</div>
            {@render checks(focus.direction.done, null)}
          {/if}
        {/if}
        <div class="list-head"><span>Parked</span>{#if focus.parked.length}<span class="n">{focus.parked.length}</span>{/if}</div>
        {#each focus.parked as idea (idea.line)}
          <div class="parked">{idea.text}</div>
        {:else}
          <p class="none small">None parked.</p>
        {/each}
        {@render adder("park", "Park an idea", { op: "park" })}
      </div>
    </div>

    {#if !focus.rule}
      <div class="setup">
        <span>Let the agents tick off what they deliver and add what you need to test.</span>
        <button class="btn primary" onclick={() => change({ op: "rule" })}>Turn on</button>
      </div>
    {/if}
  {/if}
</section>

<style>
  section { background: var(--surface); border-radius: var(--r-lg); padding: 16px 18px 12px; min-width: 0; margin-bottom: 16px; }
  .card-head { display: flex; align-items: center; gap: 10px; margin-bottom: 10px; min-width: 0; }
  h2 { color: var(--text); font-size: 14px; font-weight: 500; margin: 0; flex: none; }
  .direction { color: var(--text-3); font-size: 13px; min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .direction::before { content: "·"; color: var(--muted); margin-right: 10px; }
  .more { margin-left: auto; color: var(--muted); font-size: 12px; flex: none; }
  .more:hover { color: var(--text); }
  .card-head .grow { flex: 1; }
  .card-head .btn { flex: none; }
  .card-head .btn.on { color: var(--text); box-shadow: inset 0 0 0 1px var(--text-3); }
  .n { color: var(--muted); font-weight: 400; font-size: 12px; }
  .none { color: var(--muted); font-size: 13px; margin: 6px 0 10px; line-height: 1.5; max-width: var(--prose); }
  .none.small { font-size: 13px; margin: 4px 0; }
  .row { display: flex; gap: 8px; margin-bottom: 6px; }

  /* The stages as one line: delivered ones ticked, the one now ringed, later ones hollow. */
  .road { display: flex; align-items: center; gap: 0; overflow-x: auto; padding: 2px 0 12px; scrollbar-width: none; }
  .stop { display: inline-flex; align-items: center; gap: 8px; padding: 4px 10px; border-radius: var(--r-md); color: var(--muted); font-size: 13px; flex: none; max-width: 260px; }
  .stop:hover { background: var(--surface-hover); color: var(--text-2); }
  .stop.on { background: var(--surface-2); color: var(--text); }
  .stop.passed { color: var(--text-3); }
  .name { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .mark { width: 14px; height: 14px; border-radius: 50%; border: 1.5px solid var(--line-strong); display: grid; place-items: center; flex: none; color: var(--bg); }
  .stop.passed .mark { background: var(--muted); border-color: var(--muted); }
  .stop.active .mark { border-color: var(--accent); box-shadow: inset 0 0 0 2.5px var(--surface), inset 0 0 0 8px var(--accent); }
  .leg { width: 18px; height: 1.5px; background: var(--line-strong); flex: none; }
  .leg.passed { background: var(--muted); }

  .body { display: grid; grid-template-columns: minmax(0, 1.6fr) minmax(240px, 1fr); gap: 28px; align-items: start; }
  @container (max-width: 760px) { .body { grid-template-columns: minmax(0, 1fr); } }
  .stage-head { display: flex; align-items: center; gap: 10px; }
  .label { color: var(--muted); font-size: 12px; }
  .now { color: var(--accent); font-size: 12px; background: var(--accent-bg); border-radius: var(--r-sm); padding: 1px 8px; }
  h3 { color: var(--text); font-size: 16px; font-weight: 500; margin: 4px 0 2px; line-height: 1.35; }
  .about { color: var(--text-3); font-size: 13px; line-height: 1.55; margin: 2px 0 4px; }
  .dir-title { color: var(--text-2); font-size: 14px; margin: 0 0 2px; line-height: 1.45; }
  .sub { color: var(--muted); font-size: 12px; margin: 8px 0 2px; }
  .list-head { display: flex; align-items: center; gap: 8px; margin: 16px 0 4px; color: var(--text-2); font-size: 13px; font-weight: 500; }
  .side > .list-head:first-child { margin-top: 2px; }
  .progress { height: 3px; background: var(--surface-hover); border-radius: 3px; margin: 2px 0 6px; overflow: hidden; }
  .progress span { display: block; height: 100%; background: var(--accent); opacity: 0.7; border-radius: 3px; }

  .check { display: flex; align-items: center; gap: 12px; padding: 4px 10px; margin: 0 -10px; border-radius: var(--r-md); min-height: 34px; }
  .check:hover { background: var(--surface-hover); }
  .check .text { flex: 1; min-width: 0; color: var(--text-2); font-size: 14px; line-height: 1.45; }
  .check.done .text { color: var(--muted); text-decoration: line-through; text-decoration-color: var(--line-strong); }
  .box { width: 17px; height: 17px; border-radius: var(--r-sm); border: 1.5px solid var(--line-strong); display: grid; place-items: center; flex: none; color: var(--bg); }
  .box:hover { border-color: var(--muted); }
  .box.checked { background: var(--muted); border-color: var(--muted); }
  .hover { visibility: hidden; height: 24px; flex: none; }
  .check:hover .hover { visibility: visible; }
  .small { height: 24px; font-size: 12px; }
  .add { display: flex; align-items: center; gap: 12px; color: var(--muted); padding: 6px 0 2px; }
  .add input { flex: 1; min-width: 0; background: none; border: 0; outline: none; font-size: 13px; color: var(--text-2); }
  .arrived { display: flex; align-items: center; gap: 10px; background: var(--green-bg); color: var(--green); border-radius: var(--r-md); padding: 8px 12px; margin: 10px 0 0; font-size: 13px; }
  .arrived span { flex: 1; }
  .ideas { margin: 2px 0 4px; padding-left: 18px; color: var(--text-3); font-size: 13px; line-height: 1.6; }
  .parked { color: var(--text-3); font-size: 13px; padding: 4px 0; border-bottom: 1px solid var(--line); line-height: 1.45; }
  .parked:last-of-type { border-bottom: 0; }
  .setup { display: flex; align-items: center; gap: 12px; border-top: 1px solid var(--line); margin-top: 12px; padding-top: 12px; color: var(--text-3); font-size: 13px; }
  .setup span { flex: 1; }
</style>
