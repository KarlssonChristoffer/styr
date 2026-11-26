<!-- Settings > Key vault: every key Hub keeps, by its last four characters only. The list says what uses each one and when it was last replaced; a key opens on its own page to replace or remove it. Values live in Windows Credential Manager and never come back to the app.
     Used by Settings.svelte. -->
<script lang="ts">
  import { quiet } from "../quiet";
  import Quiet from "./Quiet.svelte";
  // Settings > Key vault: every key Hub keeps, by its last four characters only. The list says
  // what uses each one and when it was last replaced; a key opens on its own page to replace or
  // remove it. Values live in Windows Credential Manager and never come back to the app.
  import { onMount } from "svelte";
  import { api } from "../api";
  import { apis } from "../apis.svelte";
  import type { KeyUse } from "../bindings/KeyUse";
  import type { KeyVault } from "../bindings/KeyVault";
  import type { VaultKey } from "../bindings/VaultKey";
  import { ago, folderName } from "../format";
  import { hub } from "../state.svelte";
  import Icon from "./Icon.svelte";

  type State = "working" | "failing" | "missing" | "unused" | "set";

  let vault = $state<KeyVault | null>(null);
  let newName = $state("");
  let newValue = $state("");
  let replaceValue = $state("");
  let busy = $state(false);
  /** The key whose Remove was pressed once; a second press within a few seconds removes it. */
  let armed = $state<string | null>(null);
  let armTimer: ReturnType<typeof setTimeout> | undefined;
  /** What the connections said when tested right after a replace. */
  let after = $state<{ label: string; ok: boolean; message: string }[]>([]);

  const key = $derived(vault?.keys.find((k) => k.name === hub.keyOpen) ?? null);
  const nameOk = $derived(/^[A-Za-z0-9_]+$/.test(newName.trim()));
  const taken = $derived(!!vault?.keys.some((k) => k.name === newName.trim().toUpperCase()));

  async function load() {
    try {
      vault = await api.keyVault();
    } catch (e) {
      hub.fail(e);
    }
  }

  onMount(load);

  // A new page for each key: what was typed for the last one doesn't carry over.
  $effect(() => {
    void hub.keyOpen;
    replaceValue = "";
    after = [];
    armed = null;
  });

  function stateOf(k: VaultKey): State {
    if (!k.hint) return "missing";
    const apisUsing = k.uses.filter((u) => u.kind === "api");
    if (apisUsing.some((u) => u.ok === false)) return "failing";
    if (apisUsing.some((u) => u.ok === true)) return "working";
    return k.uses.length ? "set" : "unused";
  }
  const STATE: Record<State, string> = { working: "Working", failing: "Failing", missing: "Missing", unused: "Not used", set: "In use" };

  /** "3 d ago", or "just now". */
  const since = (t: number) => (ago(t) === "now" ? "just now" : `${ago(t)} ago`);
  const projectName = (path: string) => hub.projects.find((p) => p.path.toLowerCase() === path.toLowerCase())?.name || folderName(path);
  const useLabel = (u: KeyUse) => (u.kind === "project" ? projectName(u.target) : u.kind === "engine" ? `Engine ${u.label}` : u.label);

  function summary(k: VaultKey): string {
    if (!k.uses.length) return "Not used";
    const apisUsing = k.uses.filter((u) => u.kind === "api").map((u) => u.label);
    const engines = k.uses.filter((u) => u.kind === "engine").map((u) => `engine ${u.label}`);
    const projects = k.uses.filter((u) => u.kind === "project").length;
    const parts = [...apisUsing, ...engines];
    if (projects) parts.push(projects === 1 ? "1 project" : `${projects} projects`);
    return parts.join(", ") || "Hub";
  }

  function dates(k: VaultKey): string {
    if (!k.hint) return "Not in Credential Manager any more";
    const parts = [k.hint];
    if (k.changed && k.created && k.changed !== k.created) parts.push(`replaced ${since(k.changed)}`);
    else if (k.created) parts.push(`added ${since(k.created)}`);
    return parts.join(" · ");
  }

  function go(u: KeyUse) {
    if (u.kind === "api" || u.kind === "hub") {
      hub.apiOpen = u.target;
      hub.settingsSection = "apis";
    } else if (u.kind === "engine") {
      hub.settingsSection = "engines";
    } else {
      hub.settingsOpen = false;
      hub.openProject(u.target);
    }
  }

  async function add() {
    const name = newName.trim().toUpperCase();
    if (!nameOk || !newValue.trim() || taken) return;
    busy = true;
    try {
      vault = await api.createKey(name, newValue.trim());
      newName = "";
      newValue = "";
      hub.keyOpen = name;
    } catch (e) {
      hub.fail(e);
    } finally {
      busy = false;
    }
  }

  async function replace(k: VaultKey) {
    const value = replaceValue.trim();
    if (!value) return;
    busy = true;
    try {
      vault = await api.rotateKey(k.name, value);
      replaceValue = "";
      // Say straight away whether the new key works where it can be checked.
      const tests = k.uses.filter((u) => u.kind === "api");
      after = await Promise.all(
        tests.map(async (u) => {
          try {
            const t = await api.testApi(u.target);
            return { label: u.label, ok: t.ok, message: t.message };
          } catch (e) {
            return { label: u.label, ok: false, message: e instanceof Error ? e.message : String(e) };
          }
        }),
      );
      if (tests.length) {
        await quiet(apis.load(), "load");
        await load();
      }
    } catch (e) {
      hub.fail(e);
    } finally {
      busy = false;
    }
  }

  async function remove(k: VaultKey) {
    if (armed !== k.name) {
      armed = k.name;
      clearTimeout(armTimer);
      armTimer = setTimeout(() => (armed = null), 4000);
      return;
    }
    armed = null;
    try {
      await api.deleteSecret(k.name);
      hub.keyOpen = null;
      await load();
      quiet(apis.load(), "load");
    } catch (e) {
      hub.fail(e);
    }
  }

  const ICON: Record<string, "spark" | "link" | "folder" | "hub"> = { engine: "spark", api: "link", project: "folder", hub: "hub" };
</script>

{#snippet dot(s: State)}
  <span class="dot {s}"></span>
{/snippet}

{#if key}
  {@const k = key}
  {@const s = stateOf(k)}
  <button class="back" onclick={() => (hub.keyOpen = null)}><Icon name="chevron" /> Key vault</button>
  <div class="title">
    <h1 class="mono">{k.name}</h1>
    <span class="state {s}">{@render dot(s)}{STATE[s]}</span>
  </div>
  <p class="lead">
    {#if k.hint}…<span class="mono">{k.hint}</span>. In {vault?.kept_in}.
    {:else}Hub has the name, but {vault?.kept_in} no longer has the key. Paste it again below, or remove it.{/if}
  </p>

  <section>
    <div class="line slim">
      <div class="what"><div class="name">Added</div></div>
      <div class="control value">{k.created ? since(k.created) : "Before Hub kept dates"}</div>
    </div>
    <div class="line slim">
      <div class="what"><div class="name">Last replaced</div></div>
      <div class="control value">{k.changed && k.changed !== k.created ? since(k.changed) : "Never"}</div>
    </div>
    <div class="line slim">
      <div class="what"><div class="name">Last handed out</div><div class="hint">Engine, thread or Hub call.</div></div>
      <div class="control value">{k.used ? since(k.used) : "Not yet"}</div>
    </div>
  </section>

  <h2>Used by</h2>
  <section class="list">
    {#each k.uses as u (u.kind + u.target + u.label)}
      <button class="conn" onclick={() => go(u)}>
        {#if u.kind === "api"}{@render dot(u.ok === true ? "working" : u.ok === false ? "failing" : "set")}{:else}<Icon name={ICON[u.kind] ?? "link"} />{/if}
        <span class="who">
          <span class="name">{useLabel(u)}</span>
          <span class="hint">
            {#if u.kind === "api"}API connection{u.ok === false ? " · its last check failed" : u.ok ? " · working" : ""}
            {:else if u.kind === "engine"}Gets it as <span class="mono">{k.name}</span> when it starts
            {:else if u.kind === "project"}Threads in this project get it, through {u.via}
            {:else}Hub's own use, through {u.via}{/if}
          </span>
        </span>
        <Icon name="chevron" />
      </button>
    {:else}
      <Quiet text="Nothing uses it" />
    {/each}
  </section>
  {#if k.engine_key}
    <p class="note warn">A model provider's key. Never given to projects.</p>
  {/if}
  {#if k.in_env}
    <p class="note">The core's environment has a <span class="mono">{k.name}</span> too; the vault's wins.</p>
  {/if}

  <h2>Replace</h2>
  <section>
    <div class="line wrap">
      <div class="what">
        <div class="name">New key</div>
        <div class="hint">Used from the next start. Revoke the old one once this works.</div>
      </div>
      <div class="control">
        <input class="field key" type="password" placeholder="Paste the new key" autocomplete="off" spellcheck="false"
          bind:value={replaceValue} onkeydown={(e) => e.key === "Enter" && replace(k)} />
        <button class="btn small primary" disabled={busy || !replaceValue.trim()} onclick={() => replace(k)}>{busy ? "Saving…" : "Replace"}</button>
      </div>
    </div>
    {#each after as a (a.label)}
      <div class="line slim">
        <div class="what"><div class="name">{a.label}</div><div class="hint">{a.message}</div></div>
        <div class="control"><span class="state {a.ok ? 'working' : 'failing'}">{@render dot(a.ok ? "working" : "failing")}{a.ok ? "Works" : "Fails"}</span></div>
      </div>
    {/each}
  </section>

  <h2>Remove</h2>
  <section>
    <div class="line wrap">
      <div class="what">
        <div class="name">Remove from the vault</div>
        <div class="hint">
          {#if k.uses.length}{summary(k)} will have no key until you add one again.{:else}Nothing uses it.{/if}
          It is taken out of {vault?.kept_in} too.
        </div>
      </div>
      <div class="control">
        <button class="btn small danger" class:armed={armed === k.name} onclick={() => remove(k)}>{armed === k.name ? "Press again to remove" : "Remove"}</button>
      </div>
    </div>
  </section>
{:else}
  <h1>Key vault</h1>
  <p class="lead">
    Last four characters only. Claude and Codex never get these, so they stay on your subscriptions.
  </p>

  <h2>Your keys</h2>
  <section class="list">
    {#each vault?.keys ?? [] as k (k.name)}
      {@const s = stateOf(k)}
      <button class="conn" onclick={() => (hub.keyOpen = k.name)}>
        {@render dot(s)}
        <span class="who">
          <span class="name mono">{k.name}</span>
          <span class="hint">{dates(k)}</span>
        </span>
        <span class="facts">
          <span>{summary(k)}</span>
          <span>{k.used ? `Used ${since(k.used)}` : "Not handed out yet"}</span>
        </span>
        <Icon name="chevron" />
      </button>
    {:else}
      {#if vault}<Quiet text="No keys. Add one below." />{:else}<Quiet kind="loading" />{/if}
    {/each}
  </section>

  <h2>Add a key</h2>
  <section>
    <div class="line top wrap">
      <div class="what form">
        <input class="field mono" bind:value={newName} placeholder="Name, e.g. WEATHER_API_KEY" spellcheck="false" autocomplete="off" />
        <input class="field" type="password" bind:value={newValue} placeholder="The key" spellcheck="false" autocomplete="off"
          onkeydown={(e) => e.key === "Enter" && add()} />
        <div class="hint">
          {#if newName.trim() && !nameOk}A name is letters, digits and _, like an environment variable.
          {:else if taken}That name is in the vault already; open it to replace the key.
          {:else}Also the environment variable name.{/if}
        </div>
      </div>
      <div class="control">
        <button class="btn primary" disabled={busy || !nameOk || taken || !newValue.trim()} onclick={add}>Add</button>
      </div>
    </div>
  </section>

  {#if vault?.elsewhere.length}
    <h2>Outside the vault</h2>
    <p class="sub">Read only.</p>
    <section class="list">
      {#each vault.elsewhere as e (e.name)}
        <div class="change">
          <span class="hash mono">{e.hint}</span>
          <span class="subject">{e.service} <span class="dim mono">{e.name}</span></span>
          <span class="when">{e.source}</span>
        </div>
      {/each}
    </section>
  {/if}
{/if}

<style>
  h1 { color: var(--text); font-size: 20px; font-weight: 600; margin: 0; overflow-wrap: anywhere; }
  h2 { color: var(--text); font-size: 15px; font-weight: 600; margin: 30px 0 4px; }
  .lead { color: var(--text-3); margin: 8px 0 8px; font-size: 14px; }
  .sub { color: var(--muted); font-size: 13px; margin: 0 0 4px; }
  .note { color: var(--muted); font-size: 13px; margin: 8px 0 0; }
  .note.warn { color: var(--amber); }
  .back { display: inline-flex; align-items: center; gap: 4px; color: var(--muted); font-size: 13px; margin-bottom: 10px; }
  .back:hover { color: var(--text); }
  .back :global(svg) { transform: rotate(180deg); width: 14px; height: 14px; }
  .title { display: flex; align-items: center; gap: 12px; flex-wrap: wrap; }
  .state { display: inline-flex; align-items: center; gap: 6px; font-size: 13px; color: var(--text-3); background: var(--surface-hover); border-radius: var(--r-sm); padding: 2px 8px; white-space: nowrap; }
  .state.working { color: var(--green); background: var(--green-bg); }
  .state.failing, .state.missing { color: var(--red); background: var(--red-bg); }
  .dot { width: 7px; height: 7px; border-radius: 50%; background: var(--line-strong); flex: none; }
  .dot.working { background: var(--green); }
  .dot.failing, .dot.missing { background: var(--red); }
  .dot.unused { background: transparent; border: 1px solid var(--muted); }
  .line { display: flex; align-items: center; gap: 32px; padding: 14px 0; }
  .line + .line { border-top: 1px solid var(--line); }
  .line.slim { padding: 8px 0; }
  .line.top { align-items: flex-start; }
  .line.wrap { flex-wrap: wrap; row-gap: 10px; }
  .what { flex: 1; min-width: 0; display: flex; flex-direction: column; gap: 4px; }
  .line.wrap .what { min-width: 240px; }
  .name { color: var(--text); font-size: 14px; overflow-wrap: anywhere; }
  .hint { color: var(--muted); font-size: 13px; }
  .control { display: flex; align-items: center; gap: 8px; flex: none; }
  .control.value { color: var(--text-3); font-size: 13px; }
  .dim { color: var(--muted); }
  .field { width: 100%; height: 28px; padding: 0 8px; background: var(--bg); border: 1px solid var(--line-strong); border-radius: var(--r-md); font-size: 13px; outline: none; color: var(--text); }
  .field:focus { border-color: var(--accent); }
  .field.key { width: 220px; }
  .form { gap: 6px; }
  .conn { display: flex; align-items: center; gap: 12px; width: 100%; padding: 11px 10px; border-radius: var(--r-md); text-align: left; color: var(--muted); }
  .conn:hover { background: var(--surface); }
  .conn + .conn { border-top: 1px solid var(--line); }
  .conn .who { flex: 1; min-width: 0; display: flex; flex-direction: column; gap: 2px; }
  .conn .facts { display: flex; flex-direction: column; align-items: flex-end; gap: 2px; font-size: 12px; color: var(--muted); max-width: 40%; text-align: right; }
  .conn :global(svg) { width: 14px; height: 14px; flex: none; }
  .change { display: flex; align-items: center; gap: 10px; padding: 8px 0; min-height: 36px; }
  .change + .change { border-top: 1px solid var(--line); }
  .hash { color: var(--text-3); font-size: 12px; flex: none; }
  .subject { flex: 1; min-width: 0; color: var(--text-2); overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .when { color: var(--muted); font-size: 12px; flex: none; }
  .btn.small { height: 24px; padding: 0 8px; font-size: 12px; }
  .btn.armed { color: var(--red); border-color: var(--red); }
  .empty { color: var(--muted); padding: 12px 0; font-size: 13px; }
</style>
