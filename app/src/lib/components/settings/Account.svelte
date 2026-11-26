<!-- Settings page: account. Used by Settings.svelte. -->
<script lang="ts">
  import { sc, openFolder, parent, cap, guard, pct } from "./core.svelte";
  import Quiet from "../Quiet.svelte";
  import { shownPlan } from "../../plan";
  import { prefs, setPref } from "../../prefs.svelte";
  import { hub } from "../../state.svelte";
  import Field from "../Field.svelte";
  import Switch from "../Switch.svelte";
  import GithubAccount from "../GithubAccount.svelte";
  import Select from "../Select.svelte";
  import UsagePanel from "../UsagePanel.svelte";
  import ProviderLogo from "../ProviderLogo.svelte";

  const about = $derived(sc.about);
  const accounts = $derived(sc.accounts);

  // The key each engine can use instead of its subscription; the rest are other keys.
  const engineKey: Record<string, string> = { claude: "ANTHROPIC_API_KEY", codex: "OPENAI_API_KEY" };

  const otherKeys = $derived((accounts?.keys ?? []).filter((k) => !Object.values(engineKey).includes(k.name)));

  let factsOpen = $state<Record<string, boolean>>({});
</script>

<div class="head-row">
  <h1>Account and usage</h1>
  <div class="control"><Field value={prefs.displayName} placeholder={about?.user ?? "Your name"} width={160} onsave={(v) => setPref("displayName", v.trim())} /></div>
</div>
<div class="plans">
{#each accounts?.engines ?? [] as e (e.engine)}
  {@const key = accounts?.keys.find((k) => k.name === engineKey[e.engine])}
  <div class="plan">
    <div class="line">
      <div class="what ico">
        <ProviderLogo id={e.engine === "claude" ? "claude" : "openai"} size={20} />
        <div>
        <div class="name">{e.label}</div>
        <div class="hint">
          {#if e.logged_in}
            {e.method === "subscription" ? "Subscription" : "API key"}{shownPlan(e.engine, e.plan) ? ` · ${shownPlan(e.engine, e.plan)}` : ""}{e.email ? ` · ${e.email}` : ""}{e.organization && !(e.email && e.organization.includes(e.email)) ? ` · ${e.organization}` : ""}
          {:else if e.engine === "claude"}
            Not logged in. Run <code>claude</code> in a terminal and use <code>/login</code>.
          {:else}
            Not logged in. Run <code>codex login</code> in a terminal.
          {/if}
        </div>
        </div>
      </div>
      <div class="control"><span class="pill" class:ok={e.logged_in} class:off={!e.logged_in}>{e.logged_in ? "Logged in" : "Not logged in"}</span></div>
    </div>
    {#if e.source || e.command}
      <button class="link more" onclick={() => (factsOpen[e.engine] = !factsOpen[e.engine])}>{factsOpen[e.engine] ? "Hide details" : "Details"}</button>
    {/if}
    {#if factsOpen[e.engine] && (e.source || e.command)}
      <div class="facts">
        {#if e.source}<span class="k">Login</span><button class="v mono link" data-tip="Open the folder" onclick={() => openFolder(parent(e.source!))}>{e.source}</button>{/if}
        {#if e.command}<span class="k">Engine</span><span class="v mono">{e.command}</span>{/if}
      </div>
    {/if}
    {#if key}
      <div class="line">
        <div class="what"><div class="name">API key</div><div class="hint">{e.method === "api_key" ? "In use" : "Not used"}</div></div>
        <div class="control"><span class="mono dim">{key.name} {key.hint}</span><span class="pill off">{key.source}</span></div>
      </div>
    {/if}
    {#if e.engine === "claude" && e.method === "subscription"}
      <div class="usage"><UsagePanel showThreads={false} /></div>
    {:else if e.method === "subscription" && hub.others.some((o) => o.engine === e.engine)}
      <div class="usage"><UsagePanel engine={e.engine} showThreads={false} /></div>
    {/if}
  </div>
{:else}
  <section><Quiet kind="loading" /></section>
{/each}
</div>
<GithubAccount />

<h2>When a limit gets close</h2>
{#if hub.guard}
  {@const g = hub.guard}
  <section>
    <div class="line">
      <div class="what"><div class="name">Warn at</div><div class="hint">A thread says so when the 5-hour or a weekly limit is this full.</div></div>
      <div class="control"><Field value={String(g.warn_at)} width={64} onsave={(v) => pct(v, (n) => guard({ warn_at: n }))} /><span class="dim">%</span></div>
    </div>
    <div class="line">
      <div class="what"><div class="name">Switch to a cheaper model</div><div class="hint">Before the next message once a limit is this full, a thread moves to the cheaper model below. Empty means never. A model you pick yourself afterwards stays.</div></div>
      <div class="control"><Field value={g.switch_at == null ? "" : String(g.switch_at)} placeholder="Off" width={64} onsave={(v) => (v === "" ? guard({ switch_at: null }) : pct(v, (n) => guard({ switch_at: n })))} /><span class="dim">%</span></div>
    </div>
    <div class="line">
      <div class="what"><div class="name">Cheaper model</div><div class="hint">The model the switch above moves a thread to. It is only used then.</div></div>
      <div class="control">
        {#each ["sonnet", "haiku"] as m (m)}<button class="btn" class:primary={g.fallback_model === m} onclick={() => guard({ fallback_model: m })}>{cap(m)}</button>{/each}
      </div>
    </div>
    <div class="line">
      <div class="what"><div class="name">Carry on after a limit</div><div class="hint">Resumes when the limit resets.</div></div>
      <div class="control"><Switch label="Carry on after a limit" on={g.resume} onchange={(v) => guard({ resume: v })} /></div>
    </div>
    <!-- An older hubd has no `limit_switch`, so the row waits for the next version. compat-until: 2026-12-01 -->
    {#if g.limit_switch != null}
      <div class="line">
        <div class="what"><div class="name">Meanwhile, use the other subscription</div><div class="hint">Claude and Codex cover for each other. Never a paid API.</div></div>
        <div class="control">
          <Select
            label="When a limit runs out"
            value={g.limit_switch}
            options={[
              { value: "auto", label: "Switch by itself", hint: "default" },
              { value: "ask", label: "Ask me first" },
              { value: "off", label: "Just wait for the reset" },
            ]}
            onchange={(v) => guard({ limit_switch: v })}
          />
        </div>
      </div>
    {/if}
  </section>
{:else}
  <section><Quiet kind="loading" /></section>
{/if}

<h2>Other keys</h2>
<p class="vault-note">Keys you add to Hub are in the <button class="vault-link" onclick={() => { hub.keyOpen = null; hub.settingsSection = "vault"; }}>Key vault</button>.</p>
<section class="list">
  {#each otherKeys as k (k.name)}
    <div class="change">
      <span class="hash mono">{k.hint}</span>
      <span class="subject">{k.service} <span class="dim mono">{k.name}</span></span>
      <span class="when">{k.source}</span>
    </div>
  {:else}
    {#if accounts}<Quiet text="No other API keys in the core's environment" />{:else}<Quiet kind="loading" />{/if}
  {/each}
</section>

<style>
  /* @kit settings */
</style>
