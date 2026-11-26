<!-- The card that starts a thread from what Inspect mode marked: the screenshot with the marks, the text that goes with it, and where it starts. It floats over the page, so the page stays where it is, and the thread starts in the background.
     Used by App.svelte. -->
<script lang="ts">
  // The card that starts a thread from what Inspect mode marked: the screenshot with the marks,
  // the text that goes with it, and where it starts. It floats over the page, so the page stays
  // where it is, and the thread starts in the background.
  import { onMount } from "svelte";
  import { inspect } from "../inspect.svelte";
  import { describeMarks, pngAttachment, screenshot } from "../inspectSend";
  import { hub } from "../state.svelte";
  import { prefs } from "../prefs.svelte";
  import { folderName } from "../format";
  import Select from "./Select.svelte";

  let shot = $state<{ data: string; w: number; h: number } | { error: string } | null>(null);
  let message = $state("");
  let cwd = $state("");
  let starting = $state(false);
  let box = $state<HTMLTextAreaElement>();

  const projects = $derived.by(() => {
    const list = hub.projects.map((p) => ({ value: p.path, label: p.name }));
    // The folder of the thread you are in counts even when it is not a project of its own.
    if (cwd && !list.some((o) => o.value === cwd)) list.unshift({ value: cwd, label: folderName(cwd) });
    return list;
  });

  onMount(() => {
    // Hub's own project first: Inspect is mostly for Hub itself.
    const own = hub.projects.find((p) => p.name.toLowerCase() === "hub");
    cwd = own?.path ?? hub.current?.cwd ?? hub.projects[0]?.path ?? "";
    const marks = inspect.marks;
    screenshot(marks).then((s) => (shot = s));
    box?.focus();
  });

  const view = $derived(hub.view === "thread" && hub.current ? `thread "${hub.current.title}"` : hub.view);
  const extra = $derived(`${document.documentElement.dataset.theme ?? "dark"} theme · text ${prefs.textSize ?? "default"}`);
  const picture = $derived(shot && "data" in shot ? shot : null);
  const body = $derived(describeMarks(inspect.marks, inspect.groups, view, extra, !!picture));

  async function start() {
    if (!cwd || starting) return;
    starting = true;
    const text = `${message.trim() ? message.trim() + "\n\n" : ""}${body}`;
    try {
      const t = await hub.newThread(cwd, text, picture ? [pngAttachment(picture.data)] : [], {}, undefined, undefined, undefined, undefined, false);
      hub.tell(`Started a thread from Inspect: ${t.title || "new thread"}`);
      inspect.sending = false;
      inspect.clear();
    } catch {
      // `newThread` has already said what went wrong.
    } finally {
      starting = false;
    }
  }
</script>

<div class="card" data-inspect-ui role="dialog" aria-label="Start a thread from Inspect">
  <div class="top"><b>New thread</b><span>Starts in the background, this page stays</span></div>
  <div class="mid">
    <div class="thumb">
      {#if picture}<img alt="Screenshot with the marks" src={`data:image/png;base64,${picture.data}`} />
      {:else if shot}<span class="err">No screenshot: {"error" in shot ? shot.error : ""}</span>
      {:else}<span class="err">Taking a screenshot…</span>{/if}
    </div>
    <div class="chips">
      {#each inspect.marks as m (m.id)}
        <span class="chip"><i>{inspect.tag(m)}</i>{m.info.name || m.info.short}</span>
      {/each}
    </div>
  </div>
  <textarea bind:this={box} rows="3" bind:value={message} placeholder="Say what to do (empty: look at what I marked)"
    onkeydown={(e) => { if (e.key === "Enter" && (e.ctrlKey || e.metaKey)) { e.preventDefault(); start(); } }}></textarea>
  <div class="foot">
    <Select bind:value={cwd} options={projects} size="small" label="Project" placeholder="Project" />
    <span class="sp"></span>
    <button class="ghost" onclick={() => (inspect.sending = false)}>Cancel</button>
    <button class="go" disabled={!cwd || starting || !picture && !shot} onclick={start}>Start thread</button>
  </div>
</div>

<style>
  .card { position: fixed; right: calc(clamp(340px, 30vw, 460px) * var(--text-grow, 1) + 24px); bottom: 56px; z-index: 9100; width: min(540px, calc(100vw - 48px)); background: var(--surface); box-shadow: inset 0 0 0 1px var(--line-strong), 0 24px 64px rgba(0, 0, 0, 0.55); border-radius: var(--r-lg); padding: 12px; display: flex; flex-direction: column; gap: 10px; font-size: 13px; }
  .top { display: flex; justify-content: space-between; align-items: baseline; }
  .top b { font-size: 14px; font-weight: 600; }
  .top span { color: var(--muted); font-size: 12px; }
  .mid { display: flex; gap: 10px; }
  .thumb { flex: none; width: 190px; height: 110px; border-radius: var(--r-md); background: var(--bg); box-shadow: inset 0 0 0 1px var(--line-strong); overflow: hidden; display: grid; place-items: center; }
  .thumb img { width: 100%; height: 100%; object-fit: cover; object-position: top left; }
  .err { color: var(--muted); font-size: 12px; padding: 8px; text-align: center; }
  .chips { flex: 1; min-width: 0; display: flex; flex-wrap: wrap; align-content: flex-start; gap: 4px; max-height: 110px; overflow: auto; }
  .chip { display: inline-flex; gap: 6px; align-items: center; background: var(--surface-2); border-radius: var(--r-md); padding: 3px 8px; font-size: 12px; color: var(--text-3); }
  .chip i { font: 700 10px/16px var(--font); min-width: 16px; height: 16px; padding: 0 3px; box-sizing: border-box; text-align: center; border-radius: 999px; background: var(--accent); color: var(--bg); }
  textarea { resize: none; background: var(--bg); border: 1px solid var(--line-strong); color: var(--text); border-radius: var(--r-md); padding: 8px 10px; font: 13px/1.5 var(--font); outline: none; }
  textarea:focus { border-color: var(--accent); }
  .foot { display: flex; align-items: center; gap: 6px; }
  .sp { flex: 1; }
  .ghost { color: var(--text-3); padding: 4px 10px; border-radius: var(--r-md); }
  .ghost:hover { background: var(--surface-hover); color: var(--text); }
  .go { background: var(--accent); color: var(--bg); border-radius: var(--r-md); padding: 4px 12px; font-weight: 600; }
  .go:disabled { opacity: 0.4; }
</style>
