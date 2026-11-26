<!-- Your own tools (Forman, Knegarn, Aftermath…), found by the hub.toml in their folders on each machine: what each says it is doing, what waits on you, and its own commands as buttons.
     Used by WorkersView.svelte. -->
<script lang="ts">
  import { autofocus } from "../autofocus";
  import { poll } from "../poll";
  // Your own tools (Forman, Knegarn, Aftermath…), found by the hub.toml in their folders on each
  // machine: what each says it is doing, what waits on you, and its own commands as buttons.
  import { onMount } from "svelte";
  import { api } from "../api";
  import type { OwnTool } from "../bindings/OwnTool";
  import type { OwnToolAction } from "../bindings/OwnToolAction";
  import { ago } from "../format";
  import { openUrl } from "../native";
  import { noteTools } from "../ownTools.svelte";
  import { hub } from "../state.svelte";
  import Icon from "./Icon.svelte";

  type Found = { tool: OwnTool; host: string | null };

  let found = $state<Found[] | null>(null);
  /** Folders with a hub.toml that nobody approved yet; nothing in them runs. */
  let waiting = $state<{ folder: string; name: string; host: string | null }[]>([]);
  let open = $state<Record<string, boolean>>({});
  /** The action waiting for its inputs: tool key, action id, item id. */
  let asking = $state<{ key: string; action: OwnToolAction; item: string | null } | null>(null);
  let inputs = $state<Record<string, string>>({});
  let armed = $state<string | null>(null);
  let busy = $state<string | null>(null);
  /** What the last action on each tool printed. */
  let said = $state<Record<string, { ok: boolean; output: string }>>({});

  const keyOf = (f: Found) => `${f.host ?? ""}|${f.tool.folder}`;

  async function load(fresh = false) {
    const remotes = hub.hosts.filter((h) => !h.local && h.state === "online").map((h) => h.name);
    const reads = [null, ...remotes].map((host) =>
      api.ownTools(host, fresh).then(
        (tools) => (noteTools(host, tools), tools.map((tool) => ({ tool, host }))),
        () => [] as Found[],
      ),
    );
    found = (await Promise.all(reads)).flat();
    // A Hub without the call lists every tool, so there is nothing waiting.
    const asks = [null, ...remotes].map((host) =>
      api.ownToolsWaiting(host).then(
        (list) => list.map((w) => ({ ...w, host })),
        () => [],
      ),
    );
    waiting = (await Promise.all(asks)).flat();
  }

  async function allow(folder: string, host: string | null) {
    try {
      await api.allowOwnTool(folder, host);
      await load(true);
    } catch (e) {
      hub.fail(e);
    }
  }

  onMount(() => {
    load();
    const timer = poll(load, 15000);
    return () => timer();
  });

  /** A tool's own state word, coloured only when it plainly means something. */
  function tone(word: string | null | undefined): string {
    const w = (word ?? "").toLowerCase();
    if (/^(working|running|healthy|ok|clear|alive|pass|passed|completed|review|done)$/.test(w)) return "good";
    if (/^(waiting|stuck|paused|degraded|stale|warn|unknown|needs_human|escalated|open)$/.test(w)) return "warn";
    if (/^(error|failed|fail|failing|unhealthy|stopped|halted|dead)$/.test(w)) return "bad";
    return "";
  }

  function start(f: Found, action: OwnToolAction, item: string | null) {
    const key = keyOf(f);
    if (action.ask.length) {
      asking = { key, action, item };
      inputs = {};
      return;
    }
    const id = `${key}|${action.id}|${item ?? ""}`;
    if (action.danger && armed !== id) {
      armed = id;
      setTimeout(() => armed === id && (armed = null), 4000);
      return;
    }
    run(f, action, item);
  }

  async function run(f: Found, action: OwnToolAction, item: string | null) {
    const key = keyOf(f);
    busy = key;
    armed = null;
    try {
      const ran = await api.runToolAction({ folder: f.tool.folder, action: action.id, item, inputs: { ...inputs } }, f.host);
      said[key] = ran;
      asking = null;
      await load(true);
    } catch (e) {
      hub.fail(e);
    } finally {
      busy = null;
    }
  }

  function submit(e: SubmitEvent, f: Found) {
    e.preventDefault();
    if (!asking) return;
    const id = `${asking.key}|${asking.action.id}|${asking.item ?? ""}`;
    if (asking.action.danger && armed !== id) {
      armed = id;
      return;
    }
    run(f, asking.action, asking.item);
  }

  const needs = $derived((found ?? []).reduce((n, f) => n + f.tool.needs_you, 0));
</script>

{#snippet ask(f: Found, action: OwnToolAction, item: string | null)}
  {#if asking && asking.key === keyOf(f) && asking.action.id === action.id && asking.item === item}
    {@const id = `${asking.key}|${action.id}|${item ?? ""}`}
    <form class="ask" onsubmit={(e) => submit(e, f)}>
      {#each action.ask as name, i (name)}
        <input bind:value={inputs[name]} placeholder={name} use:autofocus={i === 0} onkeydown={(e) => e.key === "Escape" && (asking = null)} />
      {/each}
      <button class="btn" class:danger={action.danger} class:armed={armed === id} disabled={busy === keyOf(f)}>
        {armed === id ? `${action.label}?` : action.label}
      </button>
      <button type="button" class="icon" data-tip="Cancel" onclick={() => (asking = null)}><Icon name="x" size={12} /></button>
    </form>
  {/if}
{/snippet}

{#snippet button(f: Found, action: OwnToolAction, item: string | null)}
  {@const id = `${keyOf(f)}|${action.id}|${item ?? ""}`}
  <button class="btn" class:danger={action.danger} class:armed={armed === id} data-tip={action.about ?? ""} disabled={busy === keyOf(f)}
    onclick={() => start(f, action, item)}>{armed === id ? `${action.label}?` : action.label}</button>
{/snippet}

{#if waiting.length}
  <h4>Not allowed yet <span class="label">{waiting.length}</span></h4>
  {#each waiting as w (`${w.host ?? ""}|${w.folder}`)}
    <div class="wait">
      <span class="title"><span class="name">{w.name}{#if w.host}<span class="host">{w.host}</span>{/if}</span><span class="sub" data-tip={w.folder}>{w.folder}</span></span>
      <button class="btn" data-tip="Let Hub and its agents run this folder's commands" onclick={() => allow(w.folder, w.host)}>Allow</button>
    </div>
  {/each}
{/if}

{#if found && found.length}
  <h4>Your tools <span class="label">{found.length}{#if needs} · <span class="needs">{needs} waiting on you</span>{/if}</span></h4>
  {#each found as f (keyOf(f))}
    {@const t = f.tool}
    {@const key = keyOf(f)}
    {@const general = t.actions.filter((a) => !a.on)}
    <div class="tool">
      <div class="head">
        <button class="chev" class:open={open[key]} data-tip="Show lists" onclick={() => (open[key] = !open[key])}>
          <Icon name="chevron" size={13} />
        </button>
        <span class="dot {t.error ? 'bad' : tone(t.state)}"></span>
        <span class="title">
          <span class="name">{t.name}{#if f.host}<span class="host">{f.host}</span>{/if}{#if t.state}<span class="tag {tone(t.state)}">{t.state}</span>{/if}{#if t.needs_you}<span class="tag warn">{t.needs_you} for you</span>{/if}</span>
          <span class="sub" data-tip={t.detail ?? t.about ?? t.folder}>{t.error ?? t.headline ?? t.about ?? t.folder}</span>
        </span>
        <span class="age">{ago(t.checked_at)}</span>
        <span class="acts">{#each general as a (a.id)}{@render button(f, a, null)}{/each}</span>
      </div>
      {#each general as a (a.id)}{@render ask(f, a, null)}{/each}
      {#if said[key]}
        <button class="said mono" class:failed={!said[key].ok} data-tip="Hide" onclick={() => delete said[key]}>{said[key].output || (said[key].ok ? "Done." : "Failed, with no output.")}</button>
      {/if}
      {#each t.lists as list (list.title)}
        {#if open[key] || (list.needs_you && list.total)}
          {@const onItems = t.actions.filter((a) => a.on === list.title)}
          <div class="list">
            <span class="label">{list.title} {list.total > list.items.length ? `${list.items.length} of ${list.total}` : list.total}</span>
            {#if list.error}<p class="err">{list.error}</p>{/if}
            {#each list.items as item, i (item.id ?? i)}
              <div class="item">
                <span class="text" data-tip={item.text}>{item.text}</span>
                {#if item.state}<span class="tag {tone(item.state)}">{item.state}</span>{/if}
                {#if item.detail}<span class="sub" data-tip={item.detail}>{item.detail}</span>{/if}
                {#if item.link}<button class="icon" data-tip={item.link} onclick={() => openUrl(item.link!).catch((e) => hub.fail(e))}><Icon name="expand" size={12} /></button>{/if}
                {#if item.id}<span class="acts">{#each onItems as a (a.id)}{@render button(f, a, item.id)}{/each}</span>{/if}
              </div>
              {#if item.id}{#each onItems as a (a.id)}{@render ask(f, a, item.id)}{/each}{/if}
            {/each}
          </div>
        {/if}
      {/each}
    </div>
  {/each}
{/if}

<style>
  h4 { color: var(--text); font-size: 13px; font-weight: 500; margin: 26px 0 8px; display: flex; gap: 8px; align-items: baseline; }
  .needs { color: var(--amber); }
  .wait { display: flex; align-items: center; gap: 10px; min-height: 46px; border-bottom: 1px solid var(--line); }
  .tool { border-bottom: 1px solid var(--line); padding-bottom: 4px; }
  .head { display: flex; align-items: center; gap: 10px; min-height: 46px; padding: 4px 0; }
  .chev { display: grid; color: var(--muted); padding: 2px; border-radius: var(--r-sm); transition: transform 0.12s var(--ease); }
  .chev:hover { color: var(--text); background: var(--surface-hover); }
  .chev.open { transform: rotate(90deg); }
  .dot { width: 7px; height: 7px; border-radius: 50%; background: var(--muted); flex: none; }
  .dot.good { background: var(--green); }
  .dot.warn { background: var(--amber); }
  .dot.bad { background: var(--red); }
  .title { flex: 1; min-width: 0; display: flex; flex-direction: column; }
  .name { color: var(--text); font-size: 14px; display: flex; gap: 6px; align-items: center; }
  .host { color: var(--text-3); background: var(--surface); border-radius: var(--r-sm); padding: 0 6px; font-size: 11px; }
  .sub { color: var(--muted); font-size: 12px; white-space: nowrap; overflow: hidden; text-overflow: ellipsis; min-width: 0; }
  .age { color: var(--muted); font-size: 12px; white-space: nowrap; }
  .acts { display: inline-flex; gap: 6px; flex-wrap: wrap; justify-content: flex-end; }
  .btn { height: 24px; padding: 0 8px; font-size: 12px; }
  .btn.armed { background: var(--red-bg); color: var(--red); }
  .tag { color: var(--text-3); background: var(--surface-hover); border-radius: var(--r-sm); padding: 0 6px; font-size: 11px; white-space: nowrap; }
  .tag.good { color: var(--green); background: var(--green-bg); }
  .tag.warn { color: var(--amber); background: var(--amber-bg); }
  .tag.bad { color: var(--red); background: var(--red-bg); }
  .list { padding: 2px 0 8px 22px; display: flex; flex-direction: column; gap: 2px; }
  .item { display: flex; align-items: center; gap: 8px; min-height: 28px; padding: 0 8px; border-radius: var(--r-sm); }
  .item:hover { background: var(--surface); }
  .item .acts { visibility: hidden; }
  .item:hover .acts { visibility: visible; }
  .text { color: var(--text-2); font-size: 13px; white-space: nowrap; overflow: hidden; text-overflow: ellipsis; min-width: 0; flex: 0 1 auto; }
  .item .sub { flex: 1; }
  .icon { display: grid; color: var(--muted); padding: 3px; border-radius: var(--r-sm); }
  .icon:hover { color: var(--text); background: var(--surface-hover); }
  .ask { display: flex; gap: 6px; align-items: center; padding: 2px 8px 6px 30px; }
  .ask input { flex: 1; min-width: 0; height: 26px; padding: 0 8px; font-size: 13px; }
  .said { display: block; width: calc(100% - 22px); margin: 0 0 6px 22px; text-align: left; color: var(--text-3); font-size: 12px; white-space: pre-wrap; max-height: 120px; overflow: auto; background: var(--surface); border-radius: var(--r-sm); padding: 6px 8px; }
  .said.failed { color: var(--red); }
  .err { color: var(--red); font-size: 12px; margin: 2px 8px; }
  .label { color: var(--muted); font-size: 12px; }
  .list > .label { padding: 0 8px; }
</style>
