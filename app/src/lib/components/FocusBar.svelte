<!-- Focus mode in the sidebar: the one project every list shows, the stage it is in from its docs/FOCUS.md, how many notifications from other projects are waiting, and the way out. After leaving, what was held back shows here once, until you close it.
     Used by Sidebar.svelte. -->
<script lang="ts">
  import { quiet } from "../quiet";
  import { poll } from "../poll";
  // Focus mode in the sidebar: the one project every list shows, the stage it is in from its
  // docs/FOCUS.md, how many notifications from other projects are waiting, and the way out.
  // After leaving, what was held back shows here once, until you close it.
  import { api } from "../api";
  import type { FocusSummary } from "../bindings/FocusSummary";
  import { ago } from "../format";
  import { menu } from "../menu.svelte";
  import { threadMenu } from "../menus.svelte";
  import { prefs } from "../prefs.svelte";
  import { hub, unread } from "../state.svelte";
  import Icon from "./Icon.svelte";

  const on = $derived(prefs.focusOn);

  // The stage now, read again when a thread of the project changes state and every minute.
  let stage = $state<FocusSummary | null>(null);
  $effect(() => {
    const f = on;
    void hub.active.map((t) => t.status).join();
    stage = null;
    if (!f || f.host) return;
    const load = () => quiet(api.focusSummaries([f.cwd]).then((s) => prefs.focusOn === f && (stage = s[f.cwd] ?? null)), "focus summaries");
    load();
    const timer = poll(load, 60_000);
    return () => timer();
  });
  const stageLine = $derived.by(() => {
    if (!stage) return null;
    if (!stage.stage) return { name: "Every stage delivered", count: "" };
    const count = [stage.of ? `${stage.done}/${stage.of}` : "", stage.to_test ? `${stage.to_test} to test` : ""].filter(Boolean).join(" · ");
    return { name: `Stage ${stage.n}: ${stage.stage}`, count };
  });

  const asks = $derived(prefs.held.filter((h) => h.what === "waiting").length);
  const heldTitle = $derived(
    prefs.held.length
      ? `Held until you leave focus: ${[asks && `${asks} need you`, prefs.held.length - asks && `${prefs.held.length - asks} finished or failed`].filter(Boolean).join(", ")}`
      : "",
  );

  // Once out: each held thread as it is now, so one answered meanwhile reads as such.
  const released = $derived(
    (hub.released?.held ?? [])
      .map((h) => ({ h, t: hub.threads.find((t) => t.id === h.id) }))
      .filter((x) => !!x.t)
      .sort((a, b) => b.h.at - a.h.at),
  );
  const said = { waiting: "needed you", done: "finished", failed: "failed" } as const;
  const lasted = $derived(hub.released ? ago(hub.released.since, hub.released.until) : "");
</script>

{#if on}
  {#if hub.sidebarOpen}
    <section class="focus" aria-label="Focus mode">
      <div class="top">
        <span class="label"><Icon name="eye" size={12} />Focus</span>
        <button class="name ellipsis" data-tip={on.host ? `${on.host}: ${on.cwd}` : on.cwd} onclick={() => hub.openProject(on.cwd, on.host)}>{on.name}</button>
        <button class="leave" data-tip="Leave focus mode: every project shows again, and what was held back with it" onclick={() => hub.leaveFocus()}>Leave</button>
      </div>
      {#if stageLine}
        <button class="stage" data-tip="Open the project's focus" onclick={() => hub.openProject(on.cwd, on.host)}>
          <span class="ellipsis">{stageLine.name}</span>{#if stageLine.count}<span class="count">{stageLine.count}</span>{/if}
        </button>
      {/if}
      <button class="desk" class:on={hub.view === "thread" && !!hub.focusDesk && hub.selected === hub.focusDesk.id} data-tip="The desk: tell it what to do, and it starts and steers {on.name}'s threads" onclick={() => hub.goIn(on.cwd, on.host)}>
        <Icon name="spark" size={12} /><span class="ellipsis">Desk</span>
        {#if hub.focusDesk}<span class="dot {hub.focusDesk.status}" class:unread={unread(hub.focusDesk)}></span>{/if}
      </button>
      {#if prefs.held.length}
        <span class="held" data-tip={heldTitle}>{prefs.held.length} held from other projects</span>
      {/if}
    </section>
  {:else}
    <button class="mini" data-tip="Focus on {on.name}{prefs.held.length ? `, ${prefs.held.length} held from other projects` : ''}. Click to open it." onclick={() => hub.openProject(on.cwd, on.host)}>
      <Icon name="eye" size={16} />
    </button>
  {/if}
{:else if hub.released && hub.sidebarOpen && released.length}
  <section class="released" aria-label="Held while you focused">
    <div class="top">
      <span class="grow">While you focused on {hub.released.name}{#if lasted !== "now"} <span class="n">{lasted}</span>{/if}</span>
      <button class="close" data-tip="Got it" onclick={() => (hub.released = null)}><Icon name="x" size={12} /></button>
    </div>
    {#each released as { h, t } (h.id)}
      <button class="item" onclick={() => hub.open(h.id)} use:menu={() => threadMenu(t!)}>
        <span class="dot {t!.status}" class:unread={unread(t!)}></span>
        <span class="t ellipsis">{t!.title}</span>
        <span class="m">{said[h.what]}</span>
      </button>
    {/each}
  </section>
{/if}

<style>
  .focus, .released {
    display: flex; flex-direction: column; gap: 2px; margin: 0 0 8px; padding: 8px 8px 8px 10px;
    border-radius: var(--r-md); box-shadow: inset 0 0 0 1px var(--line-strong);
  }
  .top { display: flex; align-items: center; gap: 8px; min-width: 0; }
  .label { display: inline-flex; align-items: center; gap: 4px; flex: none; color: var(--muted); font-size: 12px; }
  .name { flex: 1; min-width: 0; text-align: left; color: var(--text); font-size: 14px; font-weight: 500; }
  .name:hover { text-decoration: underline; text-underline-offset: 3px; }
  .leave { flex: none; color: var(--muted); font-size: 12px; padding: 1px 8px; border-radius: var(--r-sm); box-shadow: inset 0 0 0 1px var(--line-strong); }
  .leave:hover { color: var(--text); background: var(--surface); }
  .stage { display: flex; align-items: baseline; gap: 6px; min-width: 0; text-align: left; color: var(--text-3); font-size: 12px; line-height: 1.4; }
  .stage:hover { color: var(--text-2); }
  .stage .count { flex: none; margin-left: auto; color: var(--muted); font-variant-numeric: tabular-nums; }
  .desk { display: flex; align-items: center; gap: 8px; min-width: 0; margin: 4px -4px 1px; padding: 4px 6px; border-radius: var(--r-sm); text-align: left; font-size: 13px; color: var(--text-2); }
  .desk:hover { background: var(--surface); color: var(--text); }
  .desk.on { background: var(--surface-hover); color: var(--text); }
  .desk .ellipsis { flex: 1; min-width: 0; }
  .held { color: var(--muted); font-size: 12px; line-height: 1.4; }
  .mini { width: 40px; height: 32px; display: grid; place-items: center; border-radius: var(--r-md); color: var(--text-2); box-shadow: inset 0 0 0 1px var(--line-strong); margin-bottom: 6px; }
  .mini:hover { background: var(--surface); color: var(--text); }
  .released .top { color: var(--text-2); font-size: 13px; padding-bottom: 2px; }
  .released .n { color: var(--muted); font-size: 12px; }
  .close { flex: none; color: var(--muted); display: grid; place-items: center; padding: 3px; border-radius: var(--r-sm); }
  .close:hover { color: var(--text); background: var(--surface-hover); }
  .item { display: flex; align-items: center; gap: 8px; min-width: 0; padding: 4px 4px; margin: 0 -4px; border-radius: var(--r-sm); text-align: left; font-size: 13px; color: var(--text-3); }
  .item:hover { background: var(--surface); color: var(--text-2); }
  .item .t { flex: 1; min-width: 0; }
  .item .m { flex: none; color: var(--muted); font-size: 12px; }
  .grow { flex: 1; min-width: 0; }
  .ellipsis { white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
</style>
