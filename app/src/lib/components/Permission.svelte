<!-- A permission question. In the transcript it is one line: where it waits, or what was answered. The open one docks in the composer, where 1, 2, 3 answer it. One that would destroy work is red, says what it would hit, and needs a second press to allow.
     Used by ComposerDock.svelte, Transcript.svelte. -->
<script lang="ts">
  import { quiet } from "../quiet";
  // A permission question. In the transcript it is one line: where it waits, or what was answered.
  // The open one docks in the composer, where 1, 2, 3 answer it. One that would destroy work is
  // red, says what it would hit, and needs a second press to allow.
  import { api } from "../api";
  import type { Reach } from "../bindings/Reach";
  import { hub } from "../state.svelte";
  import type { PermissionItem } from "../transcript";
  import Icon from "./Icon.svelte";
  import { untrack } from "svelte";
  import { typingNow } from "../typing.svelte";
  import { describeCommand } from "../describe";
  import { poll } from "../poll";

  let { item, threadId, docked = false, more = 0 }: { item: PermissionItem; threadId: string; docked?: boolean; more?: number } = $props();

  let reach = $state<Reach[] | null>(null);
  $effect(() => {
    const targets = item.risk?.targets ?? [];
    if (!docked || item.resolved || !targets.length) return;
    quiet(api.reach(threadId, targets).then((r) => (reach = r)), "reach");
  });

  let armed = $state<string | null>(null);
  let timer: ReturnType<typeof setTimeout> | undefined;

  // Shown while you were typing: held until you stop, so the keys or a click meant for your
  // message do not answer it. A click in its first moment on screen is taken as meant for what
  // was there before.
  const shownAt = Date.now();
  let held = $state(untrack(() => !item.resolved) && typingNow());
  $effect(() => {
    if (!held) return;
    return poll(() => {
      if (!typingNow()) held = false;
    }, 250);
  });

  async function answer(optionId: string | null, kind = "") {
    if (held || Date.now() - shownAt < 350) return;
    if (item.risk && kind.startsWith("allow") && armed !== optionId) {
      armed = optionId;
      clearTimeout(timer);
      timer = setTimeout(() => (armed = null), 4000);
      return;
    }
    try {
      await api.answer(threadId, item.id, optionId);
    } catch (e) {
      hub.fail(e);
    }
  }

  /** Answers with option `n`, counted from 1, as the number keys do. */
  export function pick(n: number) {
    const o = item.options[n - 1];
    if (o) answer(o.optionId, o.kind);
  }

  const total = $derived(reach?.reduce((n, r) => n + r.files, 0) ?? 0);
  // A script or heredoc is asked about whole: shown as written, under the question, not squeezed
  // beside it with its lines run together.
  const multiline = $derived(item.title.includes("\n"));
  const firstLine = $derived(item.title.split("\n")[0]);
  /** The command line asked about: the engine's own, a `ticked` one in the title, or the title. */
  const line = $derived(item.command ?? /`([^`]+)`/.exec(item.title)?.[1] ?? item.title);
  /** What it does in plain words, when Hub can tell; then the command itself folds away. */
  // A title that already says it in words ("Run `npm run migrate` on the dev database") stays.
  const plain = $derived(/`/.test(item.title) ? null : describeCommand(line));
  let showCommand = $state(false);
  const chosen = $derived(item.options.find((o) => o.optionId === item.answer));
  const allowed = $derived(!!chosen && chosen.kind.startsWith("allow"));
</script>

{#if docked}
  <div class="dock" class:risky={!!item.risk}>
    <div class="ask">
      <Icon name="shield" size={13} />
      <span class="dot" class:waiting_on_you={!item.risk} class:error={!!item.risk}></span>
      <span class="lead">Needs you</span>
      {#if plain}
        <span class="plain" data-tip={item.title}>{plain.text}{#if plain.dir}<span class="in"> in {plain.dir}</span>{/if}</span>
      {:else if !multiline}<b data-tip={item.title}>{item.title}</b>{/if}
      {#if more}<span class="more">+{more} more</span>{/if}
    </div>
    {#if plain}
      <button class="fold" onclick={() => (showCommand = !showCommand)} aria-expanded={showCommand}>
        <span class="chev" class:open={showCommand}><Icon name="chevron" size={11} /></span>{showCommand ? "Hide the command" : `Show the command${plain.steps > 1 ? ` (${plain.steps} steps)` : ""}`}
      </button>
      {#if showCommand}<pre class="command">{line.split(/\s*(?:;|&&)\s*/).join("\n")}</pre>{/if}
    {:else if multiline}<pre class="command">{item.title}</pre>{/if}
    {#if item.risk}
      <div class="risk">
        <span class="why">{item.risk.label}</span>
        {#if item.command && !item.title.includes(item.command)}<code>{item.command}</code>{/if}
        {#if reach}
          <ul>
            {#each reach as r (r.target)}
              <li>
                <code>{r.target}</code>
                {#if !r.exists}<span class="muted">not found</span>
                {:else if r.dir}<span>{r.files}{r.more ? "+" : ""} {r.files === 1 ? "file" : "files"}</span>
                {/if}
              </li>
            {/each}
          </ul>
          {#if total}
            <details>
              <summary>Show files</summary>
              <pre>{reach.flatMap((r) => r.sample).join("\n")}{reach.some((r) => r.files > r.sample.length) ? "\n…" : ""}</pre>
            </details>
          {/if}
        {/if}
      </div>
    {/if}
    <div class="options" class:held>
      {#each item.options as o, n (o.optionId)}
        <button class="option" disabled={held} class:armed={armed === o.optionId} onclick={() => answer(o.optionId, o.kind)}>
          <span class="key">{n + 1}</span>{armed === o.optionId ? `Sure? Press ${n + 1} again to ${o.name.toLowerCase()}` : o.name}
        </button>
      {/each}
      {#if held}<span class="held-note">Held while you type</span>{/if}
    </div>
  </div>
{:else if item.resolved}
  <div class="line" data-tip={item.title}>
    <Icon name="shield" size={13} />
    <span class="state" class:ok={allowed} class:no={!allowed}>{chosen ? chosen.name : "Rejected"}</span>
    {#if plain}<span class="text">{plain.text}</span>{:else}<span class="text mono">{firstLine}</span>{/if}
  </div>
{:else}
  <div class="line waiting" data-tip={item.title}>
    <Icon name="shield" size={13} />
    <span class="dot" class:waiting_on_you={!item.risk} class:error={!!item.risk}></span>
    {#if plain}<span class="text">{plain.text}</span>{:else}<span class="text mono">{firstLine}</span>{/if}
    <span class="where">Answer below</span>
  </div>
{/if}

<style>
  .line { display: flex; align-items: center; gap: 8px; color: var(--muted); font-size: 13px; margin: 6px 0; min-width: 0; }
  .line .text { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; min-width: 0; color: var(--text-3); font-size: 12px; }
  .line .state { flex: none; }
  .line .state.ok { color: var(--green); }
  .line .state.no { color: var(--red); }
  .line.waiting .where { flex: none; color: var(--amber); }
  .mono { font-family: var(--mono); }

  .dock { padding: 2px 0 8px; margin-bottom: 8px; border-bottom: 1px solid var(--line); }
  .ask { display: flex; align-items: center; gap: 8px; color: var(--muted); min-width: 0; }
  .ask .lead { color: var(--text-2); flex: none; }
  .ask b { color: var(--text); font-weight: 500; font-family: var(--mono); font-size: 13px; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; min-width: 0; }
  .plain { color: var(--text); overflow: hidden; text-overflow: ellipsis; white-space: nowrap; min-width: 0; }
  .plain .in { color: var(--muted); }
  .fold { display: inline-flex; align-items: center; gap: 4px; margin: 4px 0 0 21px; padding: 1px 4px; border-radius: var(--r-sm); color: var(--muted); font-size: 12px; }
  .chev { display: inline-flex; transition: transform var(--dur-fast) var(--ease); }
  .chev.open { transform: rotate(90deg); }
  .fold:hover { color: var(--text-2); background: var(--surface-hover); }
  .more { color: var(--muted); font-size: 12px; flex: none; margin-left: auto; padding-right: 8px; }
  .command { margin: 6px 0 0 21px; color: var(--text); font-size: 13px; max-height: 200px; }
  .risk { margin: 8px 0 0 21px; font-size: 13px; color: var(--text-2); display: flex; flex-direction: column; gap: 4px; }
  .why { color: var(--red); }
  code { font-family: var(--mono); font-size: 12px; color: var(--text); }
  ul { list-style: none; margin: 0; padding: 0; }
  li { display: flex; gap: 10px; }
  .muted { color: var(--muted); }
  summary { cursor: pointer; color: var(--muted); font-size: 12px; }
  pre { margin: 4px 0 0; font-family: var(--mono); font-size: 12px; color: var(--text-3); white-space: pre-wrap; max-height: 180px; overflow: auto; }
  .options { display: flex; flex-direction: column; gap: 1px; margin: 8px 8px 0 12px; }
  .option { display: flex; align-items: center; gap: 10px; text-align: left; padding: 4px 8px; border-radius: var(--r-md); color: var(--text-3); font-size: 13px; }
  .option:hover { background: var(--surface-hover); color: var(--text); }
  .option.armed { color: var(--red); background: var(--red-bg); }
  .key { display: inline-grid; place-items: center; width: 18px; height: 18px; border-radius: var(--r-sm); border: 1px solid var(--line-strong); color: var(--muted); font-size: 11px; font-family: var(--mono); flex: none; }
  .options.held .option { opacity: 0.5; }
  .held-note { color: var(--muted); font-size: 12px; margin: 2px 0 0 8px; }
</style>
