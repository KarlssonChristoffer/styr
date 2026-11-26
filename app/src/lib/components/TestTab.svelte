<!-- The project's "To test" scripts from docs/FOCUS.md, in stage order, to go through one by one: Start opens what to do and what to look for, and ticking one writes it back to the file, the same as the focus card does.
     Used by ProjectView.svelte, TestWindow.svelte. -->
<script lang="ts">
  import Quiet from "./Quiet.svelte";
  import { api } from "../api";
  import type { Check } from "../bindings/Check";
  import type { Focus } from "../bindings/Focus";
  import { hub } from "../state.svelte";
  import { popOut, inTauri } from "../native";
  import { parseScript } from "../testScripts";
  import Icon from "./Icon.svelte";

  // The project's "To test" scripts from docs/FOCUS.md, in stage order, to go through one by one:
  // Start opens what to do and what to look for, and ticking one writes it back to the file, the
  // same as the focus card does.
  let {
    path,
    host = null,
    focus,
    onfocus,
    onfill,
    onprompt,
    window: inWindow = false,
  }: {
    path: string;
    host?: string | null;
    focus: Focus;
    onfocus: (f: Focus) => void;
    /** Puts text in the project's composer to finish before sending. */
    onfill: (text: string) => void;
    /** Starts a thread in the project with the text in its box. */
    onprompt: (text: string) => void;
    /** This is the window of its own, not the project page. */
    window?: boolean;
  } = $props();

  const stages = $derived(
    focus.stages
      .map((s, n) => ({ s, n, items: s.test.map((c) => ({ c, ...parseScript(c.text) })) }))
      .filter((g) => g.items.length > 0),
  );
  const all = $derived(stages.flatMap((g) => g.items));
  const doneCount = $derived(all.filter((x) => x.c.done).length);

  let open = $state<number | null>(null);
  let busy = $state(false);

  async function tick(c: Check) {
    if (busy) return;
    busy = true;
    try {
      onfocus(await api.editFocus({ path, op: "tick", line: c.line, done: !c.done }, host));
    } catch (e) {
      hub.fail(e);
    } finally {
      busy = false;
    }
  }
  async function finish(c: Check) {
    await tick(c);
    if (open === c.line) {
      // On to the next script that is still open.
      const next = all.find((x) => !x.c.done && x.c.line !== c.line);
      open = next ? next.c.line : null;
    }
  }
  function broke(title: string) {
    onfill(`I tested "${title}" from the To test list in docs/FOCUS.md and it does not work. What happened: `);
  }
  let copied = $state<number | null>(null);
  async function copy(line: number, text: string) {
    try {
      await navigator.clipboard.writeText(text);
      copied = line;
      setTimeout(() => (copied = copied === line ? null : copied), 1500);
    } catch (e) {
      hub.fail(e);
    }
  }
  const aside = () => popOut({ thread: "", kind: "tests", id: JSON.stringify({ path, host }) }).catch((e) => hub.fail(e));
  const firstOpen = $derived(all.find((x) => !x.c.done)?.c.line ?? null);
</script>

<div class="tests" class:win={inWindow}>
  {#if !all.length}
    <Quiet text="Nothing to test" />
  {:else}
    <div class="head">
      <h2>To test</h2>
      <span class="n">{doneCount} of {all.length} done</span>
      <span class="grow"></span>
      {#if firstOpen !== null && open !== firstOpen}<button class="btn small" onclick={() => (open = firstOpen)}>Start next</button>{/if}
      {#if !inWindow && inTauri}<button class="btn small" onclick={aside} data-tip="Opens the list in a window of its own, for another screen. Go and Prompt still work on this one."><Icon name="popout" size={12} /> Own window</button>{/if}
    </div>
    <div class="progress"><span style:width="{(doneCount / all.length) * 100}%"></span></div>

    {#each stages as g (g.n)}
      <section>
        <div class="stage"><span class="label">Stage {g.n + 1}</span><span class="name">{g.s.title.replace(/^Stage \d+:\s*/, "")}</span><span class="n">{g.items.filter((x) => x.c.done).length}/{g.items.length}</span></div>
        {#each g.items as x (x.c.line)}
          <div class="script" class:done={x.c.done} class:open={open === x.c.line}>
            <div class="row">
              <button class="box" class:checked={x.c.done} onclick={() => tick(x.c)} data-tip={x.c.done ? "Mark as not done" : "Mark as done"}>
                {#if x.c.done}<Icon name="check" size={12} />{/if}
              </button>
              <button class="title" onclick={() => (open = open === x.c.line ? null : x.c.line)}>{x.title}</button>
              {#if !x.c.done && open !== x.c.line}<button class="btn small" onclick={() => (open = x.c.line)}>Start</button>{/if}
            </div>
            {#if open === x.c.line}
              <div class="detail">
                <div class="k">Do</div>
                <p>{x.todo}</p>
                {#if x.look}<div class="k">Look for</div><p>{x.look}</p>{/if}
                {#if x.go.length}
                  <div class="k">Go to</div>
                  <div class="chips">{#each x.go as g (g.label)}<button class="btn small" onclick={() => hub.goTo(g.to)}>{g.label}</button>{/each}</div>
                {/if}
                {#if x.prompt}
                  <div class="k">Prompt</div>
                  <p class="prompt">{x.prompt}</p>
                  <div class="chips">
                    <button class="btn small" onclick={() => onprompt(x.prompt)} data-tip="Opens a new thread in this project with the prompt in the box, to read before sending">Start a thread with it</button>
                    <button class="btn small" onclick={() => copy(x.c.line, x.prompt)}>{copied === x.c.line ? "Copied" : "Copy"}</button>
                  </div>
                {/if}
                <div class="actions">
                  {#if !x.c.done}<button class="btn small primary" onclick={() => finish(x.c)}>Done, next</button>{/if}
                  <button class="btn small" onclick={() => broke(x.title)}>It does not work</button>
                  <button class="btn small ghost-btn" onclick={() => (open = null)}>Close</button>
                </div>
              </div>
            {/if}
          </div>
        {/each}
      </section>
    {/each}
  {/if}
</div>

<style>
  .tests { max-width: 760px; }
  .tests.win { max-width: none; padding: 14px 18px; }
  .head { display: flex; align-items: center; gap: 10px; }
  h2 { color: var(--text); font-size: 14px; font-weight: 500; margin: 0; }
  .grow { flex: 1; }
  .n { color: var(--muted); font-size: 12px; }
  .progress { height: 3px; background: var(--surface-hover); border-radius: 3px; margin: 8px 0 14px; overflow: hidden; }
  .progress span { display: block; height: 100%; background: var(--accent); opacity: 0.7; border-radius: 3px; }
  section { margin-bottom: 18px; }
  .stage { display: flex; align-items: baseline; gap: 8px; margin: 0 0 4px; }
  .label { color: var(--muted); font-size: 12px; }
  .name { color: var(--text-2); font-size: 13px; flex: 1; min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .script { border-radius: var(--r-md); padding: 4px 10px; margin: 0 -10px; }
  .script:hover, .script.open { background: var(--surface-hover); }
  .row { display: flex; align-items: center; gap: 12px; min-height: 34px; }
  .title { flex: 1; min-width: 0; text-align: left; color: var(--text-2); font-size: 14px; line-height: 1.45; }
  .script.done .title { color: var(--muted); text-decoration: line-through; text-decoration-color: var(--line-strong); }
  .box { width: 17px; height: 17px; border-radius: var(--r-sm); border: 1.5px solid var(--line-strong); display: grid; place-items: center; flex: none; color: var(--bg); }
  .box:hover { border-color: var(--muted); }
  .box.checked { background: var(--muted); border-color: var(--muted); }
  .detail { padding: 2px 0 10px 29px; }
  .k { color: var(--muted); font-size: 12px; margin: 6px 0 2px; }
  .detail p { margin: 0; color: var(--text-2); font-size: 14px; line-height: 1.55; }
  .chips { display: flex; flex-wrap: wrap; gap: 6px; margin-top: 4px; }
  .prompt { background: var(--surface); border-radius: var(--r-md); padding: 8px 10px; font-size: 13px !important; }
  .actions { display: flex; gap: 8px; margin-top: 12px; }
  .small { height: 24px; font-size: 12px; flex: none; }
</style>
