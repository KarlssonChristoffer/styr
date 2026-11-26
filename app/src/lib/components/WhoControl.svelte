<!-- Who answers: the provider's mark picks the provider, the name picks the model. Both open their
     menu above the box (`ModelPicker` is the list of models). Used by Composer.svelte. -->
<script lang="ts">
  import { pop } from "../popover";
  import { hub } from "../state.svelte";
  import type { SettingValue } from "../bindings/SettingValue";
  import { modelLabel, type ConfigOption, type Session } from "../session";
  import { SWITCH_WARNING, type ModelRow } from "../modelRows";
  import { choiceName, configOf } from "../composerConfig";
  import {
    engineProvider,
    engines,
    favorites,
    modelsFor,
    harnessLabel,
    tabsOf,
    hostEngines,
    loadEngines,
    loadHostEngines,
    loadReady,
    notReady,
    fixEngine,
    missingProviders,
    providerOf,
    loadHarnessFacts,
  } from "../providers.svelte";
  import ModelPicker from "./ModelPicker.svelte";
  import ProviderMark from "./ProviderMark.svelte";
  import Icon from "./Icon.svelte";

  let {
    session,
    engine,
    host,
    inThread = false,
    busy = false,
    menu = $bindable(),
    onengine,
    onerror,
    onchoose,
    onfocus,
  }: {
    session: Session;
    engine: string;
    host?: string | null;
    inThread?: boolean;
    busy?: boolean;
    /** Which menu of the bar is open; the mark and the name open and close theirs. */
    menu: string | null;
    /** Moves to another engine, with `model` picked on it when known. */
    onengine?: (engine: string, model?: SettingValue) => Promise<void> | void;
    onerror?: (e: unknown) => void;
    onchoose: (option: ConfigOption, value: SettingValue, close?: boolean) => void;
    /** Puts the cursor back in the box. */
    onfocus: () => void;
  } = $props();

  const model = $derived(configOf(session).model);

  const provider = $derived(providerOf(engine, model ? choiceName(model) + " " + String(model.value) : undefined));


  $effect(() => {
    loadEngines();
  });
  // A thread on another machine switches among that machine's engines, not this one's.
  $effect(() => {
    if (inThread && host) loadHostEngines(host);
  });
  const offered = $derived(inThread && host ? (hostEngines[host] ?? []) : engines.names);
  // Engines that can't start on that machine (not installed, not signed in there) are shown but
  // can't be picked, with why, instead of failing after the switch.
  $effect(() => {
    if (menu === "model" || menu === "provider") loadReady(host);
  });
  // The catalog gives the picker each engine's harness and, for one on a provider's key that has
  // not run here yet, that provider's models.
  $effect(() => {
    if (menu === "model" || menu === "provider") loadHarnessFacts();
  });
  const unready = $derived(notReady[host ?? ""] ?? {});
  const machineName = $derived(host || hub.localHost || "this machine");
  const unreadyWhy = (e: string) => `${engineProvider(e).name} (${e}) is not ready on ${machineName}: ${unready[e].detail}.`;
  /** Fixes an engine that is not ready right from the picker: a terminal on that machine signs it in. */
  async function fixUnready(e: string) {
    const u = unready[e];
    if (!u) return;
    menu = null;
    try {
      const said = await fixEngine(host, machineName, u);
      if (said) hub.tell(said);
    } catch (err) {
      hub.fail(err);
    }
  }
  /** Says why an engine can't be picked, with the button that fixes it. */
  function sayUnready(e: string) {
    const u = unready[e];
    hub.fail(new Error(unreadyWhy(e)), u?.action ? { label: u.action, run: () => fixUnready(e) } : undefined);
  }
  const engineNames = $derived(offered.length ? (offered.includes(engine) ? offered : [engine, ...offered]) : [engine]);
  const modelsOf = (e: string) => (e === engine ? model : modelsFor(e));
  // One tab per provider: a model that several harnesses run (DeepSeek in Hermes and in Claude
  // Code) is one tab with a head per harness. A provider with a single harness shows no harness.
  const tabs = $derived(tabsOf(engineNames));
  const tabEngines = (e: string) => tabs.find((t) => t.engines.includes(e))?.engines ?? [e];
  /** The harness's name when other harnesses run the same provider here; else nothing to choose. */
  const viaOf = (e: string) => (tabEngines(e).length > 1 ? harnessLabel(e) : "");
  const modelName = (o: ConfigOption) => {
    const c = o.choices.find((c) => c.value === o.value);
    return c ? modelLabel(c) : String(o.value);
  };
  /** Starred models some configured engine still offers. */
  const hasFavorites = $derived(
    favorites.keys.some((k) => {
      const at = k.indexOf(":");
      const e = k.slice(0, at);
      return engineNames.includes(e) && !!modelsOf(e)?.choices.some((c) => c.value === k.slice(at + 1));
    }),
  );
  /** The tab the picker opens on. */
  let pickerAt = $state("");
  let models = $state<ModelPicker>();
  /** Opens on the favorites when there are any, else on `at`. */
  function openModels(at?: string) {
    if (menu === "model") return (menu = null);
    pickerAt = at ?? (hasFavorites ? "favorites" : engine);
    menu = "model";
  }
  async function pick(r: ModelRow) {
    menu = null;
    if (r.engine !== engine && unready[r.engine]) {
      sayUnready(r.engine);
    } else if (r.engine !== engine) {
      try {
        await onengine?.(r.engine, r.choice?.value);
      } catch (e) {
        onerror?.(e);
      }
    } else if (r.choice && model) {
      onchoose(model, r.choice.value);
    }
    queueMicrotask(() => onfocus());
  }
  /** Another provider can't take over mid-turn; the picker still closes, and says why. */
  function lockedPick() {
    menu = null;
    onerror?.(new Error("Let the turn finish before switching provider."));
  }
  const canSwitch = $derived(!!onengine && !busy);
  function manageEngines() {
    menu = null;
    hub.openSettings("engines");
  }
  /** Ctrl Shift 1-9 picks the nth row of the open picker. */
  export function pickNth(n: number) {
    return models?.pickNth(n);
  }
  export { openModels };
</script>

<!-- Who answers: the provider's mark picks the provider, the name picks the model. -->
<div class="menu-wrap who">
  <button
    class="mark-btn"
    class:open={menu === "provider"}
    data-tip="{provider.name} · pick the provider"
    onclick={() => (menu = menu === "provider" ? null : "provider")}
  ><ProviderMark id={provider.id} size={15} /></button>
  <button class="pill model-pill" class:open={menu === "model"} data-tip="Pick the model (Ctrl Shift M)" onclick={() => openModels()}>
    <span class="label">{model ? modelName(model) : provider.name}{#if viaOf(engine)}<span class="via">{viaOf(engine)}</span>{/if}</span><Icon name="down" size={12} />
  </button>
  {#if menu === "provider"}
    <div class="menu providers" transition:pop|global role="menu">
      <div class="menu-title">Provider{#if !canSwitch && onengine} · finish the turn to switch{/if}</div>
      {#each tabs as t (t.key)}
        {@const several = t.engines.length > 1}
        <!-- A provider that several harnesses run is one row: it opens the picker on its tab,
             where the harness is chosen with the model. -->
        {@const e = t.engines.includes(engine) ? engine : t.key}
        {@const p = engineProvider(e)}
        {@const known = modelsOf(e)}
        <button
          class="option row two"
          class:active={t.engines.includes(engine)}
          disabled={!several && e !== engine && !canSwitch}
          class:unready={!several && e !== engine && !!unready[e]}
          onclick={() => (several ? openModels(t.key) : e === engine ? openModels(e) : unready[e]?.action && canSwitch ? fixUnready(e) : pick({ engine: e, choice: null }))}
        >
          <ProviderMark id={p.id} size={16} />
          <span class="grow lines">
            <span class="line1">{p.name}{#if !several}<span class="eng mono">{e}</span>{/if}</span>
            <span class="line2">{!several && e !== engine && unready[e] ? `${unready[e].short} on ${machineName}${unready[e].action ? ` · click to ${unready[e].action.toLowerCase()}` : ""}` : known ? choiceName(known) : "Models show after its first start"}</span>
          </span>
          {#if t.engines.includes(engine)}<span class="tick"><Icon name="check" size={14} /></span>{/if}
        </button>
      {/each}
      {#each missingProviders() as p (p.id)}
        <button class="option row two missing" onclick={manageEngines}>
          <ProviderMark id={p.id} size={16} />
          <span class="grow lines"><span class="line1">{p.name}</span><span class="line2">Not set up · add it as an engine</span></span>
          <Icon name="external" size={13} />
        </button>
      {/each}
      {#if inThread && onengine}
        <p class="blurb"><span class="warn">New session.</span> {SWITCH_WARNING}</p>
      {/if}
    </div>
  {/if}
  {#if menu === "model"}
    <ModelPicker
      bind:this={models}
      at={pickerAt}
      {engine}
      {model}
      {engineNames}
      {modelsOf}
      {tabs}
      {tabEngines}
      {viaOf}
      {unready}
      {unreadyWhy}
      {canSwitch}
      switching={inThread && !!onengine}
      onpick={pick}
      onlocked={lockedPick}
      onfix={fixUnready}
      onmanage={manageEngines}
      onclose={() => {
        menu = null;
        onfocus();
      }}
    />
  {/if}
</div>

<style>
  /* @kit menus */
  .who { display: inline-flex; align-items: center; border-radius: var(--r-md); }
  .who:hover { background: var(--surface-hover); }
  .mark-btn { height: 28px; width: 26px; display: grid; place-items: center; border-radius: var(--r-md) 0 0 var(--r-md); padding-left: 4px; }
  .mark-btn:hover, .mark-btn.open { background: var(--line-strong); }
  .model-pill { border-radius: 0 var(--r-md) var(--r-md) 0; padding-left: 4px; }
  .who:hover .model-pill:not(:hover) { background: none; }
  /* Which harness runs it, only where another harness runs the same provider. */
  .via { color: var(--muted); font-size: 0.92em; margin-left: 6px; }
  .via::before { content: "· "; }
</style>
