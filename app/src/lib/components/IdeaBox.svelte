<!-- The idea inbox (Ctrl I): say or write an idea at any time and Hub puts it in the focus of the project it is about, under the stage it fits or under Parked. The box stays open for the next one; below it, the last ideas say where they went and can be moved or taken back.
     Used by App.svelte. -->
<script lang="ts">
  import { quiet } from "../quiet";
  import { modal } from "../pop";
  import { onDestroy, onMount } from "svelte";
  import { api } from "../api";
  import type { InboxIdea } from "../bindings/InboxIdea";
  import type { IdeaPlace } from "../bindings/IdeaPlace";
  import { ago } from "../format";
  import { hub } from "../state.svelte";
  import { ComposerVoice } from "../composerVoice.svelte";
  import Icon from "./Icon.svelte";
  import Select from "./Select.svelte";

  // The idea inbox (Ctrl I): say or write an idea at any time and Hub puts it in the focus of the
  // project it is about, under the stage it fits or under Parked. The box stays open for the
  // next one; below it, the last ideas say where they went and can be moved or taken back.
  let { onclose }: { onclose: () => void } = $props();

  let text = $state("");
  let box = $state<HTMLTextAreaElement>();
  let keeping = $state(false);
  let places = $state<IdeaPlace[]>([]);
  let moving = $state<string | null>(null);

  // The project you are looking at is the guess when the idea does not say.
  const here = $derived(
    hub.view === "project" && hub.project && !hub.projectHost ? hub.project
    : hub.view === "thread" && hub.current && !hub.current.host ? hub.current.cwd
    : null,
  );
  const recent = $derived(hub.ideas.slice(0, 8));

  onMount(() => {
    box?.focus();
    quiet(api.ideaPlaces().then((p) => (places = p)), "idea places");
  });

  async function keep() {
    const said = text.trim();
    if (!said || keeping) return;
    keeping = true;
    try {
      const idea = await api.captureIdea({ text: said, here: here ?? undefined });
      if (!hub.ideas.some((i) => i.id === idea.id)) hub.ideas.unshift(idea);
      text = "";
    } catch (e) {
      hub.fail(e);
    } finally {
      keeping = false;
      box?.focus();
    }
  }

  function where(i: InboxIdea) {
    if (i.state === "sorting") return "Finding where it goes…";
    if (i.state === "unsorted") return "No project guessed: pick one";
    return `${i.name} · ${i.stage ? `${i.stage} ideas` : "Parked"}`;
  }

  async function move(i: InboxIdea, value: string) {
    moving = null;
    const [project, stage] = JSON.parse(value) as [string, string | null];
    try {
      await api.moveIdea(i.id, { project, stage: stage ?? undefined });
    } catch (e) {
      hub.fail(e);
    }
  }

  async function drop(i: InboxIdea) {
    try {
      await api.dropIdea(i.id);
      hub.ideas = hub.ideas.filter((x) => x.id !== i.id);
    } catch (e) {
      hub.fail(e);
    }
  }

  function open(i: InboxIdea) {
    if (!i.project) return;
    onclose();
    hub.openProject(i.project, null);
  }

  // ---- Voice: the same live dictation as the message box (words appear as you speak, final
  // text replaces them, the small model tidies at the end), with the project as hint. ----
  const voice = new ComposerVoice({
    get text() {
      return text;
    },
    set text(v: string) {
      text = v;
    },
    get caret() {
      return box?.selectionStart ?? text.length;
    },
    element: () => box,
    cwd: () => here ?? undefined,
    onerror: (e) => hub.fail(e),
  });
  onDestroy(() => voice.destroy());

  function keydown(e: KeyboardEvent) {
    if (e.key === "Escape") {
      e.preventDefault();
      e.stopPropagation();
      onclose();
    } else if (e.key === "Enter" && !e.shiftKey && !e.isComposing) {
      e.preventDefault();
      voice.settle().then(keep);
    } else if ((e.ctrlKey || e.metaKey) && e.key === "d") {
      e.preventDefault();
      voice.toggle();
    }
  }
</script>

<div role="presentation" class="backdrop" in:modal|global out:modal|global={{ out: true }} onclick={(e) => e.target === e.currentTarget && onclose()}>
  <div class="dialog" role="dialog" aria-modal="true" aria-label="Idea inbox">
    <div class="field">
      <Icon name="bulb" size={16} />
      <textarea
        bind:this={box}
        bind:value={text}
        rows="1"
        placeholder={voice.state === "recording" ? "Listening… press the mic or Ctrl D when done" : "An idea, for any project. Hub puts it in the right focus."}
        onkeydown={keydown}
      ></textarea>
      <button class="mic" class:on={voice.state === "recording"} disabled={voice.state === "transcribing"} onclick={() => voice.toggle()}
        title={voice.state === "recording" ? "Stop (Ctrl D)" : "Say it (Ctrl D)"} aria-label="Say the idea">
        <Icon name="mic" size={15} />
      </button>
    </div>
    <div class="hint">
      {#if voice.note}{voice.note}{:else if voice.state === "transcribing"}Writing out what you said…{:else}Enter keeps it · Shift Enter for a new line · Esc closes{/if}
    </div>

    {#if recent.length}
      <ul class="list">
        {#each recent as i (i.id)}
          <li class:unsorted={i.state === "unsorted"}>
            <div class="text">{i.text}</div>
            <div class="meta">
              {#if i.project}
                <button class="where" onclick={() => open(i)} title={i.why ?? (i.by === "name" ? "The idea names it" : i.by === "here" ? "The page you were on" : "")}>{where(i)}</button>
              {:else}
                <span class="where" class:busy={i.state === "sorting"}>{where(i)}</span>
              {/if}
              <span class="when">{ago(i.at)}</span>
              {#if i.state !== "sorting"}
                {#if moving === i.id}
                  <Select size="small" autoopen placeholder="Move to…" label="Move to" onchange={(v) => move(i, v)} onclose={() => (moving = null)}
                    options={places.flatMap((p) => [
                      { value: JSON.stringify([p.path, null]), label: "Parked", group: p.name },
                      ...p.stages.map((s, n) => ({ value: JSON.stringify([p.path, s]), label: `${s}${p.active === n ? " (now)" : ""}`, group: p.name })),
                    ])} />
                {:else}
                  <button class="act" onclick={() => (moving = i.id)}>{i.state === "unsorted" ? "Pick a project" : "Move"}</button>
                {/if}
                <button class="act" onclick={() => drop(i)} title="Take it out of the focus file again">{i.project ? "Undo" : "Remove"}</button>
              {/if}
            </div>
          </li>
        {/each}
      </ul>
    {/if}
  </div>
</div>

<style>
  .backdrop { position: fixed; inset: 0; background: rgba(0, 0, 0, 0.35); z-index: 50; display: flex; justify-content: center; align-items: flex-start; padding-top: 12vh; }
  .dialog { width: min(600px, calc(100vw - 32px)); max-height: 76vh; overflow-y: auto; background: var(--surface-2); border: 1px solid var(--line-strong); border-radius: var(--r-lg); box-shadow: 0 24px 60px rgba(0, 0, 0, 0.5); padding: 14px 16px 12px; }
  .field { display: flex; align-items: flex-start; gap: 10px; color: var(--muted); }
  .field > :global(svg) { margin-top: 4px; flex: none; }
  textarea { flex: 1; min-width: 0; resize: none; field-sizing: content; max-height: 40vh; background: none; border: 0; outline: none; font: inherit; font-size: 15px; line-height: 1.5; color: var(--text); padding: 1px 0; }
  textarea::placeholder { color: var(--muted); }
  .mic { flex: none; color: var(--text-3); padding: 4px; border-radius: var(--r-md); }
  .mic:hover { color: var(--text); background: var(--surface-hover); }
  .mic.on { color: var(--red); box-shadow: inset 0 0 0 1px var(--red); }
  .hint { color: var(--muted); font-size: 12px; margin: 4px 0 0 26px; }
  .list { list-style: none; margin: 12px 0 0; padding: 8px 0 0; border-top: 1px solid var(--line); display: flex; flex-direction: column; gap: 2px; }
  li { padding: 6px 8px; border-radius: var(--r-md); }
  li:hover { background: var(--surface-hover); }
  .text { color: var(--text-2); font-size: 14px; line-height: 1.45; overflow-wrap: anywhere; }
  .meta { display: flex; flex-wrap: wrap; align-items: center; gap: 4px 10px; margin-top: 2px; font-size: 12px; color: var(--muted); }
  .where { color: var(--text-3); text-align: left; }
  button.where:hover { color: var(--text); text-decoration: underline; }
  .where.busy { color: var(--muted); font-style: italic; }
  .unsorted .where { color: var(--amber); }
  .act { color: var(--muted); opacity: 0; }
  li:hover .act, li:focus-within .act, .unsorted .act { opacity: 1; }
  .act:hover { color: var(--text); text-decoration: underline; }
</style>
