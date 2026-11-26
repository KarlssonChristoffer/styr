<!-- The menu behind your name in the sidebar: a quick look at your account and plan, and the way into Settings. Everything else is found in Settings, by its page or by Find a setting.
     Used by Rail.svelte, Sidebar.svelte. -->
<script lang="ts">
  import { quiet } from "../quiet";
  import { poll } from "../poll";
  import { pop } from "../popover";
  // The menu behind your name in the sidebar: a quick look at your account and plan, and the way
  // into Settings. Everything else is found in Settings, by its page or by Find a setting.
  import { onMount } from "svelte";
  import { api } from "../api";
  import type { EngineAccount } from "../bindings/EngineAccount";
  import { tick } from "svelte";
  import { switchCard, THEMED_ENGINES } from "../cardSwitch";
  import { ago, resetOf, shortVersion } from "../format";
  import { shownPlan } from "../plan";
  import { prefs, setPref } from "../prefs.svelte";
  import { hub, type SettingsSection } from "../state.svelte";
  import CountUp from "./CountUp.svelte";
  import Icon from "./Icon.svelte";
  import ProviderMark from "./ProviderMark.svelte";
  import { engineProvider } from "../providers.svelte";

  /** `foot`: opened from the usage strip at the foot of the sidebar, so it spans the sidebar
   *  instead of growing out of the avatar on the rail. */
  let { onclose, foot = false }: { onclose: () => void; foot?: boolean } = $props();

  let accounts = $state<EngineAccount[]>([]);
  let menu = $state<HTMLElement>();
  let card = $state<HTMLElement>();
  let checking = $state(false);
  let switching = false;
  let now = $state(Date.now());
  let open = true;

  onMount(() => {
    const clock = poll(() => (now = Date.now()), 30_000);
    quiet(api.accounts().then((a) => (accounts = a.engines.filter((e) => e.logged_in))), "accounts");
    // Codex's limits are a file read, so they are fresh each time the menu opens.
    quiet(api.usage(false).then((u) => hub.setUsage(u)), "usage");
    const away = (e: PointerEvent) => {
      if (!menu?.contains(e.target as Node) && !(e.target as Element).closest?.(".who, .me-strip")) onclose();
    };
    const key = (e: KeyboardEvent) => e.key === "Escape" && onclose();
    window.addEventListener("pointerdown", away);
    window.addEventListener("keydown", key);
    return () => {
      open = false;
      clock();
      window.removeEventListener("pointerdown", away);
      window.removeEventListener("keydown", key);
    };
  });

  function go(section?: SettingsSection, spot: string | null = null) {
    hub.openSettings(section, spot);
    onclose();
  }

  /** The card leads to this subscription's limits; only its two buttons do something else. */
  const usage = () => go("account", accounts.find((a) => a.engine === sub.engine)?.label ?? null);

  const name = $derived(prefs.displayName || hub.host?.hostname || "You");
  const subs = $derived(hub.subscriptions);
  const sub = $derived(hub.subscription);
  const index = $derived(subs.findIndex((s) => s.engine === sub.engine));
  const next = $derived(subs[(index + 1) % subs.length]);
  const account = $derived(accounts.find((a) => a.engine === sub.engine) ?? null);
  // The plan's name is the provider's own, tidied; see plan.ts.
  const plan = $derived(shownPlan(sub.engine, sub.plan?.plan ?? account?.plan));

  /** Changes the card to another subscription, the way Settings > Appearance says. */
  async function flip(step = 1) {
    if (subs.length < 2 || switching || !card) return;
    switching = true;
    const to = subs[(index + step + subs.length) % subs.length];
    await switchCard(card, prefs.cardSwitch, step, async () => {
      setPref("menuAccount", to.engine === "claude" ? "" : to.engine);
      await tick();
    });
    switching = false;
  }

  /** Asks again: Claude runs `/usage`, which takes a while; the others read their logs. */
  async function refresh() {
    if (checking) return;
    checking = true;
    try {
      let u = await api.usage(sub.engine === "claude" ? "force" : false);
      hub.setUsage(u);
      for (let tries = 0; u.refreshing && open && tries < 60; tries++) {
        await new Promise((r) => setTimeout(r, 1500));
        u = await api.usage(false);
        hub.setUsage(u);
      }
    } catch (e) {
      hub.fail(e);
    } finally {
      checking = false;
    }
  }

  /** The most lines any subscription needs, which every one of them gets. */
  const rows = $derived(Math.max(1, ...subs.map((s) => s.plan?.limits.length ?? 1)));
  const age = $derived(sub.plan ? ago(sub.plan.checked_at, now) : null);
  const level = (pct: number) => (pct >= 95 ? "full" : pct >= 80 ? "warn" : "");
  const u = $derived(hub.update);
  const version = $derived(u ? (shortVersion(u.version)) : null);
</script>

<div class="menu" transition:pop|global class:foot bind:this={menu} role="menu">
  <div class="who-line">
    <div class="name ellipsis">{name}</div>
  </div>
  <div class="deck" class:decked={prefs.cardSwitch === "deck" && subs.length > 1}>
  <div class="peek"></div>
  <div
    class="card"
    data-style={prefs.cardSwitch}
    bind:this={card}
    role="group"
    aria-label="{sub.label} limits"
    onwheel={(e) => {
      if (subs.length > 1 && Math.abs(e.deltaX) > Math.abs(e.deltaY) && Math.abs(e.deltaX) > 20) flip(e.deltaX > 0 ? 1 : -1);
    }}
  >
    {#if prefs.cardTheme !== "off"}
      <!-- One layer per company, crossfading as you switch; it sits behind and never takes room. -->
      <div class="fx" data-theme={prefs.cardTheme} aria-hidden="true">
        {#each THEMED_ENGINES as e (e)}<i class={e} class:on={sub.engine === e}></i>{/each}
      </div>
    {/if}
    <div role="presentation" class="card-head" onclick={(e) => !(e.target as Element).closest("button") && usage()}>
      <span class="provider"><ProviderMark id={engineProvider(sub.engine).id} size={14} /><span class="pname" data-word>{sub.label}</span>{#if plan}<span class="plan" data-word>{plan}</span>{/if}</span>
      <button
        class="age"
        class:checking
        data-tip={sub.engine === "claude"
          ? "Ask Claude Code for your limits now"
          : `Read again. ${sub.label} updates its limits each time it answers`}
        onclick={refresh}
      >
        {#if age}<span class="ago" data-word data-tip="Checked {age === "now" ? "just now" : `${age} ago`}">{age}</span>{/if}<Icon name="restart" size={12} />
      </button>
      {#if subs.length > 1}
        <button class="flip" data-tip="Show {next.label}'s limits" onclick={() => flip()}>
          <!-- Every name takes the same place, so the button keeps its width as you switch. -->
          {#if subs.length > 2}
            <!-- Three or more: the next one's mark, so the head stays one line. -->
            <ProviderMark id={engineProvider(next.engine).id} size={13} />
          {:else}
            <!-- Narrow cards show the mark instead of the name (see .card-head). -->
            <span class="mark"><ProviderMark id={engineProvider(next.engine).id} size={13} /></span>
            <span class="names">{#each subs as s (s.engine)}<span class:on={s.engine === next.engine}>{s.label}</span>{/each}</span>
          {/if}<Icon name="swap" size={13} />
        </button>
      {/if}
    </div>
    <button class="limits" data-tip="Account and usage" onclick={usage}>
      {#if sub.balance}
        <span class="limit balance"><span class="label ellipsis" data-word>Balance left</span><span class="pct" data-word>{sub.balance}</span></span>
      {:else if sub.plan?.limits.length}
        <!-- Keyed by place, so a switch changes the same rows and their bars can slide. -->
        {#each sub.plan.limits as l, i (i)}
          {@const when = resetOf(l)}
          <span class="limit" data-tip={when ? `${l.label} · resets ${when}` : l.label}>
            <span class="label ellipsis" data-word>{l.label}</span>
            <span class="bar"><span class={level(l.percent)} style:width="{Math.max(l.percent, 1)}%"></span></span>
            <span class="pct" data-word><CountUp value={l.percent} /></span>
          </span>
        {/each}
      {:else}
        <span class="asof" data-word>{sub.note ?? "No limits read yet. Press refresh to ask."}</span>
      {/if}
      <!-- Empty rows up to the longest list, so the card and its buttons stay put as you switch. -->
      {#each { length: rows - Math.max(sub.plan?.limits.length ?? 0, 1) } as _, i (i)}<span class="limit" aria-hidden="true"></span>{/each}
    </button>
  </div>
  </div>
  <div class="sep"></div>
  <button class="item" role="menuitem" onclick={() => go("account")}><Icon name="user" /><span>Account and usage</span></button>
  <button class="item" role="menuitem" onclick={() => go()}><Icon name="gear" /><span>Settings</span><kbd>Ctrl ,</kbd></button>
  <button class="item" role="menuitem" onclick={() => go(hub.freshNotes || !u || u.state === "idle" ? "whatsnew" : "updates")}>
    <Icon name="log" /><span>What's new</span>
    {#if hub.freshNotes}<span class="end fresh">New</span>
    {:else if u?.state === "ready"}<span class="end fresh">Update ready</span>
    {:else if u?.state === "failed"}<span class="end warn">Update failed</span>
    {:else if version}<span class="end mono">{version}</span>{/if}
  </button>
</div>

<style>
  .menu {
    position: absolute; left: 0; bottom: calc(100% + 6px); z-index: 30;
    width: 268px; padding: 6px;
    background: var(--surface-2); border: 1px solid var(--line-strong); border-radius: var(--r-lg);
    box-shadow: 0 12px 32px rgba(0, 0, 0, 0.45);
  }
  /* From the usage strip: as wide as the sidebar's foot, so it stays inside the sidebar. */
  .menu.foot { left: -4px; right: -4px; width: auto; }
  .who-line { padding: 6px 10px 8px; }
  .name { color: var(--text); font-size: 14px; font-weight: 500; display: flex; align-items: center; gap: 8px; }
  .plan { color: var(--muted); font-size: 12px; font-weight: 400; }
  .deck { position: relative; margin: 0 4px 2px; }
  .deck.decked { margin-bottom: 8px; }
  /* Deck shows the edge of the next card under this one. */
  .peek { display: none; position: absolute; left: 6px; right: 6px; bottom: -5px; height: 12px; border-radius: 0 0 var(--r-md) var(--r-md); background: var(--bg-sidebar); border: 1px solid var(--line-strong); border-top: 0; opacity: 0.7; }
  .decked .peek { display: block; }
  .card { position: relative; z-index: 1; overflow: hidden; border-radius: var(--r-md); background: var(--bg-sidebar); }
  .card { isolation: isolate; }
  /* The backdrop: each company's colours, moving slowly enough to read as light, not as motion. */
  .fx { --clay: 217, 119, 87; --sand: 204, 160, 112; --sky: 110, 140, 255; --iris: 150, 120, 255; --mint: 120, 210, 190; position: absolute; inset: 0; z-index: -1; pointer-events: none; }
  :global(:root[data-theme="cappuccino"]) .fx, :global(:root[data-theme="paper"]) .fx, :global(:root[data-theme="sage"]) .fx { opacity: 0.7; }
  .fx i { position: absolute; inset: 0; overflow: hidden; opacity: 0; transition: opacity 420ms var(--ease); }
  .fx i.on { opacity: 1; }
  .fx i::before, .fx i::after { content: ""; position: absolute; }

  /* Glow: two soft lights drifting behind the numbers. */
  .fx[data-theme="glow"] i::before, .fx[data-theme="glow"] i::after { width: 170px; height: 170px; border-radius: 50%; filter: blur(38px); }
  .fx[data-theme="glow"] .claude::before { background: rgba(var(--clay), 0.3); left: -60px; top: 20px; animation: drift-a 18s var(--ease-move) infinite alternate; }
  .fx[data-theme="glow"] .claude::after { background: rgba(var(--sand), 0.16); right: -70px; top: -80px; animation: drift-b 23s var(--ease-move) infinite alternate; }
  .fx[data-theme="glow"] .codex::before { background: rgba(var(--sky), 0.24); left: -50px; top: -70px; animation: drift-b 16s var(--ease-move) infinite alternate; }
  .fx[data-theme="glow"] .codex::after { background: rgba(var(--iris), 0.2); right: -60px; top: 30px; animation: drift-a 21s var(--ease-move) infinite alternate; }
  @keyframes drift-a { to { transform: translate(60px, -30px) scale(1.15); } }
  @keyframes drift-b { to { transform: translate(-50px, 40px) scale(0.9); } }

  /* Material: Claude is warm paper with ink lines drifting; Codex a dot grid a light passes over. */
  .fx[data-theme="material"] .claude { background: radial-gradient(120% 90% at 0% 100%, rgba(var(--clay), 0.16), transparent 60%); }
  .fx[data-theme="material"] .claude::before {
    inset: 0; opacity: 0.22; mix-blend-mode: overlay;
    background-image: url("data:image/svg+xml,%3Csvg xmlns='http://www.w3.org/2000/svg' width='160' height='160'%3E%3Cfilter id='n'%3E%3CfeTurbulence type='fractalNoise' baseFrequency='.9' numOctaves='2'/%3E%3C/filter%3E%3Crect width='100%25' height='100%25' filter='url(%23n)'/%3E%3C/svg%3E");
  }
  .fx[data-theme="material"] .claude::after {
    left: 0; right: 0; bottom: -6px; height: 60px; opacity: 0.5; background-size: 300px 60px; animation: ink 26s linear infinite;
    background-image: url("data:image/svg+xml,%3Csvg xmlns='http://www.w3.org/2000/svg' width='300' height='60'%3E%3Cg fill='none' stroke='rgb(217,119,87)' stroke-opacity='.35'%3E%3Cpath d='M0 40 C50 20 100 55 150 35 S250 20 300 40'/%3E%3Cpath d='M0 50 C60 35 110 62 160 45 S250 32 300 50' stroke-opacity='.2'/%3E%3C/g%3E%3C/svg%3E");
  }
  @keyframes ink { to { background-position: 300px 0; } }
  .fx[data-theme="material"] .codex { background: radial-gradient(color-mix(in srgb, var(--text) 9%, transparent) 1px, transparent 1.2px) 0 0 / 9px 9px; }
  .fx[data-theme="material"] .codex::before { inset: 0; background: radial-gradient(100% 80% at 100% 0%, rgba(var(--sky), 0.1), transparent 60%); }
  .fx[data-theme="material"] .codex::after {
    top: -40%; bottom: -40%; width: 90px; left: -120px; transform: rotate(18deg); filter: blur(6px);
    background: linear-gradient(90deg, transparent, rgba(var(--sky), 0.2), rgba(var(--mint), 0.14), transparent);
    animation: sweep 7s var(--ease-move) infinite;
  }
  @keyframes sweep { 0%, 55% { left: -120px; } 100% { left: 330px; } }

  /* Edge: a lit top edge that wanders, and a faint wash below it. */
  .fx[data-theme="edge"] .claude { background: linear-gradient(180deg, rgba(var(--clay), 0.09), transparent 55%); }
  .fx[data-theme="edge"] .codex { background: linear-gradient(180deg, rgba(var(--sky), 0.08), transparent 55%); }
  .fx[data-theme="edge"] i::before { left: 0; right: 0; top: 0; height: 1px; background-size: 200% 100%; animation: edge 9s linear infinite; }
  .fx[data-theme="edge"] .claude::before { background-image: linear-gradient(90deg, transparent, rgba(var(--clay), 0.9), rgba(var(--sand), 0.6), transparent 60%); }
  .fx[data-theme="edge"] .codex::before { background-image: linear-gradient(90deg, transparent, rgba(var(--sky), 0.9), rgba(var(--iris), 0.6), transparent 60%); }
  @keyframes edge { from { background-position: 200% 0; } to { background-position: -200% 0; } }
  @media (prefers-reduced-motion: reduce) { .fx i::before, .fx i::after { animation: none; } }
  .card[data-style="morph"] .bar span, .card[data-style="ticker"] .bar span { transition: width var(--dur-move) var(--ease-move); }
  /* The head is one line that never clips the name or the buttons at its right edge, whatever the
     text size or plan badge. What gives way is dropped whole, never cut to "10…" or "Cl…": first
     the time since the check (its tooltip still says it), then the other subscription's name
     (its mark stays). The widths are in em of the head's own text, which is what grows with the
     text size, so the steps come at the same place at every size. */
  .age { margin-left: auto; flex: none; min-width: 22px; display: flex; align-items: center; gap: 4px; height: 22px; padding: 0 4px; border-radius: var(--r-sm); color: var(--muted); font-size: 11px; font-variant-numeric: tabular-nums; }
  .age:hover { background: var(--surface-hover); color: var(--text); }
  .age:focus-visible { outline: 1px solid var(--line-strong); outline-offset: 0; }
  .age :global(svg) { color: var(--text-3); flex: none; }
  .ago { white-space: nowrap; }
  @container head (max-width: 19em) { .ago { display: none; } }
  @container head (max-width: 14em) { .names { display: none; } .flip .mark { display: flex; } }
  .age.checking :global(svg) { animation: spin 0.9s linear infinite; }
  @keyframes spin { to { transform: rotate(360deg); } }
  /* One line: see .age for what gives way when it is too narrow. */
  .card-head { container: head / inline-size; font-size: 12px; display: flex; flex-wrap: nowrap; align-items: center; gap: 0 6px; white-space: nowrap; min-height: 30px; padding: 0 4px 0 8px; cursor: pointer; }
  .flip .mark { display: none; }
  .card:hover:not(:has(.age:hover, .flip:hover)) { background: var(--surface-hover); }
  .provider { flex: 0 0 auto; min-width: 0; max-width: 100%; display: flex; align-items: center; gap: 8px; color: var(--text-2); font-size: 12px; font-weight: 500; }
  .provider :global(svg), .plan { flex: none; }
  .pname { min-width: 0; overflow: hidden; text-overflow: ellipsis; }
  .flip { flex: none; display: flex; align-items: center; gap: 4px; height: 22px; padding: 0 6px; border-radius: var(--r-sm); color: var(--muted); font-size: 12px; }
  .flip:hover { background: var(--surface-hover); color: var(--text); }
  /* The app's accent ring is too loud on a button this small; a hairline is enough. */
  .flip:focus-visible { outline: 1px solid var(--line-strong); outline-offset: 0; }
  .flip :global(svg) { color: var(--text-3); }
  /* The label column is as wide as the longest label, then shrinks (to an ellipsis, never under
     6em) only when the card is too narrow for it and a bar. Each row shares the columns, so the bars line up. */
  .limits { display: grid; grid-template-columns: minmax(6em, max-content) minmax(1.5em, 1fr) auto; column-gap: 8px; row-gap: 6px; width: 100%; padding: 2px 8px 8px; border-radius: 0 0 var(--r-md) var(--r-md); text-align: left; }
  .asof { color: var(--text-3); font-size: 11px; }
  .limit, .asof { height: 16px; line-height: 16px; }
  .names { display: grid; }
  .limits > .asof { grid-column: 1 / -1; }
  .limit.balance { grid-template-columns: 1fr auto; }
  .names > span { grid-area: 1 / 1; visibility: hidden; text-align: right; }
  .names > span.on { visibility: visible; }
  .limit { grid-column: 1 / -1; display: grid; grid-template-columns: subgrid; align-items: center; font-size: 12px; color: var(--muted); }
  .bar { height: 4px; border-radius: 2px; background: var(--line-strong); overflow: hidden; }
  .bar span { display: block; height: 100%; background: var(--text-3); border-radius: 2px; }
  .bar span.warn { background: var(--amber); }
  .bar span.full { background: var(--red); }
  .pct { min-width: 4ch; text-align: right; color: var(--text-3); font-variant-numeric: tabular-nums; }
  .item {
    display: flex; align-items: center; gap: 10px; width: 100%;
    height: 32px; padding: 0 10px; border-radius: var(--r-md);
    color: var(--text-2); font-size: 14px; text-align: left;
  }
  .item:hover { background: var(--surface-hover); color: var(--text); }
  .item :global(svg) { flex: none; color: var(--text-3); }
  kbd, .end { margin-left: auto; color: var(--muted); font-size: 12px; font-family: var(--font); display: flex; }
  .end.mono { font-family: var(--mono); }
  .end.fresh { color: var(--accent); background: var(--accent-bg); border-radius: var(--r-sm); padding: 0 6px; }
  .end.warn { color: var(--amber); }
  .sep { height: 1px; background: var(--line-strong); margin: 4px 4px; }
  .ellipsis { white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
</style>
