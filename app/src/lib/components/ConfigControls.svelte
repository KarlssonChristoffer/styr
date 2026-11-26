<!-- The composer's thinking-effort and permission-mode controls: a pill each, and the menu it opens.
     Used by Composer.svelte. -->
<script lang="ts">
  import { quietly } from "../quiet";
  import { pop } from "../popover";
  import { api } from "../api";
  import type { SettingValue } from "../bindings/SettingValue";
  import type { HarnessSetting } from "../bindings/HarnessSetting";
  import { effortLevels, isOn, ROUTE, toggled, type ConfigOption, type Session } from "../session";
  import { harnessFacts, loadHarnessFacts, routeEfforts } from "../providers.svelte";
  import { cap, choiceName, configOf, isDanger, modeIcon } from "../composerConfig";
  import Icon from "./Icon.svelte";

  let {
    session,
    engine,
    host,
    menu = $bindable(),
    onchoose,
    onflash,
  }: {
    session: Session;
    engine: string;
    host?: string | null;
    /** Which menu of the bar is open; the pills here open and close theirs. */
    menu: string | null;
    onchoose: (option: ConfigOption, value: SettingValue, close?: boolean) => void;
    onflash: (what: string) => void;
  } = $props();

  const config = $derived(configOf(session));
  const mode = $derived(config.mode);
  const model = $derived(config.model);
  const effort = $derived(config.effort);
  const toggles = $derived(config.toggles);
  const others = $derived(config.others);

  // The thinking levels the provider's models really have, when Hub runs the harness on a
  // provider (DeepSeek: Off, Low, High, Max); a harness may list more than the model has.
  $effect(() => {
    if (model?.id === ROUTE) loadHarnessFacts();
  });
  const real = $derived(model?.id === ROUTE ? routeEfforts(model.value) : []);
  const levels = $derived(effort ? effortLevels(effort, real) : []);
  /** The effort as filled bars out of the levels there are; `auto` when the engine decides. */
  const effortBars = $derived.by(() => {
    if (!effort) return null;
    if (levels.length < 2) return null;
    const at = levels.findIndex((c) => c.value === effort.value);
    const auto = at < 0;
    const n = (auto ? levels.findIndex((c) => c.recommended) : at) + 1;
    return { n, of: Math.min(levels.length, 5), auto, name: auto ? "Auto" : effortName(effort) };
  });
  function effortName(o: ConfigOption) {
    return effortLevels(o, real).find((c) => c.value === o.value)?.name ?? choiceName(o);
  }

  // A harness that runs on a provider with thinking levels but offers no thinking setting per
  // thread (Hermes before its config options) still thinks at its own default, which Hub can
  // change: shown as the same control, saying it is for every thread of that harness.
  const harness = $derived(harnessFacts.harnessOf[session.engine ?? engine] ?? "");
  let shared = $state<{ harness: string; host: string; setting: HarnessSetting | null } | null>(null);
  const fallback = $derived(!effort && real.length > 1 && !!harnessFacts.settings[harness]);
  $effect(() => {
    if (!fallback || (shared?.harness === harness && shared.host === (host ?? ""))) return;
    const want = { harness, host: host ?? "" };
    shared = { ...want, setting: null };
    api
      .harnessSettings(harness, host)
      .then((h) => {
        if (shared?.harness === want.harness && shared.host === want.host) shared = { ...want, setting: h.problem ? null : (h.settings.find((x) => x.thinking) ?? null) };
      })
      .catch(quietly("harness settings"));
  });
  const sharedSetting = $derived(fallback ? (shared?.setting ?? null) : null);
  /** Where the shared default stands among the provider's levels; the provider's default when unset. */
  const sharedAt = $derived.by(() => {
    if (!sharedSetting) return -1;
    const at = real.findIndex((r) => r.value === sharedSetting.value);
    return at >= 0 ? at : real.findIndex((r) => r.default);
  });
  async function chooseShared(value: string) {
    if (!sharedSetting) return;
    menu = null;
    const before = sharedSetting.value;
    sharedSetting.value = value;
    try {
      const h = await api.setHarnessSetting(harness, sharedSetting.key, value, host);
      if (shared) shared = { ...shared, setting: h.settings.find((x) => x.thinking) ?? null };
    } catch (e) {
      sharedSetting.value = before;
      onflash(`Could not change it: ${e instanceof Error ? e.message : e}`);
    }
  }
  const effortLabel = $derived(
    effortBars?.name ?? (effort ? choiceName(effort) : toggles.some(isOn) ? toggles.filter(isOn).map((o) => o.name).join(" · ") : "Options"),
  );
</script>

{#if effort || toggles.length || others.length}
  <span class="sep"></span>
  <div class="menu-wrap">
    <button class="pill" class:open={menu === "effort"} data-tip="{effort?.name ?? 'Options'} (Ctrl Shift E)" onclick={() => (menu = menu === "effort" ? null : "effort")}>
      <Icon name="brain" size={14} />
      <span class="label">{cap(effortLabel)}</span>
      <Icon name="down" size={12} />
    </button>
    {#if menu === "effort"}
      <div class="menu effort" transition:pop|global role="menu">
        {#if effort}
          <div class="menu-title">{effort.name}</div>
          <p class="blurb">Higher thinks longer.</p>
          {#each levels as c, k (c.value)}
            <button class="option row" class:active={c.value === effort.value} data-tip={c.description} onclick={() => onchoose(effort, c.value, false)}>
              <span class="bars" aria-hidden="true">
                {#each Array(Math.min(levels.length, 5)) as _, j (j)}<i class:on={j < Math.min(k, 4) + (real.length && levels[0]?.value === "none" ? 0 : 1)} style:height="{4 + j * 2}px"></i>{/each}
              </span>
              <span class="grow">{cap(c.name)}
                {#if c.recommended}<span class="badge">{real.length ? "Default" : "Recommended"}</span>{/if}
                {#if c.costly}<span class="badge warn">3.5× or more usage</span>{/if}
              </span>
              {#if c.value === effort.value}<span class="tick"><Icon name="check" size={14} /></span>{/if}
            </button>
          {/each}
        {/if}
        {#if toggles.length}
          {#if effort}<div class="divider"></div>{/if}
          {#each toggles as o (o.id)}
            <button class="option row" data-tip={o.description} onclick={() => onchoose(o, toggled(o), false)} aria-pressed={isOn(o)}>
              <span class="grow">{o.name}</span><span class="switch" class:on={isOn(o)}></span>
            </button>
          {/each}
        {/if}
        {#each others as o (o.id)}
          <div class="divider"></div>
          <div class="menu-title">{o.name}</div>
          {#each o.choices as c (c.value)}
            <button class="option row" class:active={c.value === o.value} data-tip={c.description} onclick={() => onchoose(o, c.value, false)}>
              <span class="grow">{c.name}</span>{#if c.value === o.value}<span class="tick"><Icon name="check" size={14} /></span>{/if}
            </button>
          {/each}
        {/each}
      </div>
    {/if}
  </div>
{/if}

{#if sharedSetting && sharedAt >= 0}
  <span class="sep"></span>
  <div class="menu-wrap">
    <button class="pill" class:open={menu === "shared-effort"} data-tip="Thinking for every {harness} thread" onclick={() => (menu = menu === "shared-effort" ? null : "shared-effort")}>
      <Icon name="brain" size={14} />
      <span class="label">{real[sharedAt]?.name}</span>
      <Icon name="down" size={12} />
    </button>
    {#if menu === "shared-effort"}
      <div class="menu effort" transition:pop|global role="menu">
        <div class="menu-title">Thinking</div>
        <p class="blurb">One level for all threads.</p>
        {#each real as c, k (c.value)}
          <button class="option row" class:active={k === sharedAt} data-tip={c.what} onclick={() => chooseShared(c.value)}>
            <span class="bars" aria-hidden="true">
              {#each Array(Math.min(real.length, 5)) as _, j (j)}<i class:on={j < k + (real[0]?.value === "none" ? 0 : 1)} style:height="{4 + j * 2}px"></i>{/each}
            </span>
            <span class="grow">{c.name}{#if c.default}<span class="badge">Default</span>{/if}</span>
            {#if k === sharedAt}<span class="tick"><Icon name="check" size={14} /></span>{/if}
          </button>
        {/each}
      </div>
    {/if}
  </div>
{/if}

{#if mode}
  <span class="sep"></span>
  <div class="menu-wrap">
    <button
      class="pill"
      class:danger={isDanger(mode.value)}
      class:open={menu === "mode"}
      data-tip="{mode.description ?? mode.name} · Shift Tab cycles, Ctrl Shift A opens"
      onclick={() => (menu = menu === "mode" ? null : "mode")}
    >
      <Icon name={modeIcon(mode.value)} size={14} /><span class="label">{cap(choiceName(mode))}</span><Icon name="down" size={12} />
    </button>
    {#if menu === "mode"}
      <div class="menu modes" transition:pop|global role="menu">
        <div class="menu-title">What it may do without asking</div>
        {#each mode.choices as c (c.value)}
          <button class="option row two" class:active={c.value === mode.value} class:danger={isDanger(c.value)} onclick={() => onchoose(mode, c.value)}>
            <Icon name={modeIcon(c.value)} size={15} />
            <span class="grow lines">
              <span class="line1">{cap(c.name)}</span>
              {#if c.description}<span class="line2">{c.description}</span>{/if}
            </span>
            {#if c.value === mode.value}<span class="tick"><Icon name="check" size={14} /></span>{/if}
          </button>
        {/each}
      </div>
    {/if}
  </div>
{/if}

<style>
  /* @kit menus */
</style>
