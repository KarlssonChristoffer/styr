<!-- Settings > Engines > Providers: the providers you have a key for, tried against their API, and one Add provider that lists the rest. Every harness that runs on a provider offers its models in the model picker once the key is there (`routes.rs`).
     Used by ApiConnections.svelte, Settings.svelte. -->
<script lang="ts">
  import { quiet } from "../quiet";
  import Quiet from "./Quiet.svelte";
  // Settings > Engines > Providers: the providers you have a key for, tried against their API,
  // and one Add provider that lists the rest. Every harness that runs on a provider offers its
  // models in the model picker once the key is there (`routes.rs`).
  import type { Snippet } from "svelte";
  import { api } from "../api";
  import type { EngineCatalog } from "../bindings/EngineCatalog";
  import type { Provider } from "../bindings/Provider";
  import type { ProviderTest } from "../bindings/ProviderTest";
  import { openUrl } from "../native";
  import { hub } from "../state.svelte";
  import ProviderLogo from "./ProviderLogo.svelte";
  import Icon from "./Icon.svelte";

  /** `after` draws more rows at the end of the rack (Local models). */
  let { onchange, after }: { onchange?: () => void; after?: Snippet } = $props();

  let catalog = $state<EngineCatalog | null>(null);
  let failed = $state("");
  /** The provider whose key is being typed: a new one from Add provider, or one to replace. */
  let editing = $state("");
  let adding = $state(false);
  let key = $state("");
  let saving = $state(false);
  let tests = $state<Record<string, ProviderTest | "running">>({});
  let query = $state("");
  let search = $state<HTMLInputElement>();

  const mine = $derived(catalog?.providers.filter((p) => p.has_key) ?? []);
  const others = $derived(catalog?.providers.filter((p) => !p.has_key) ?? []);
  const shown = $derived(
    others.filter((p) => `${p.label} ${p.what} ${p.models.join(" ")}`.toLowerCase().includes(query.trim().toLowerCase())),
  );
  const picked = $derived(catalog?.providers.find((p) => p.id === editing) ?? null);

  async function load() {
    try {
      catalog = await api.engineCatalog(null);
      failed = "";
    } catch (e) {
      failed = e instanceof Error ? e.message : String(e);
    }
  }

  let tried = false;
  $effect(() => {
    load().then(() => {
      // Each provider is tried once when the page opens, so a dead key shows without a click.
      if (tried) return;
      tried = true;
      for (const p of mine) test(p);
    });
  });

  async function test(p: Provider) {
    tests = { ...tests, [p.id]: "running" };
    try {
      tests = { ...tests, [p.id]: await api.testProvider(p.id) };
    } catch (e) {
      tests = { ...tests, [p.id]: { ok: false, message: e instanceof Error ? e.message : String(e), models: [] } };
    }
  }

  function usedBy(p: Provider): string[] {
    return catalog?.harnesses.filter((h) => h.providers.includes(p.id)).map((h) => h.label) ?? [];
  }

  async function save() {
    const p = picked;
    if (!p || !key.trim()) return;
    saving = true;
    try {
      // A key it already has under another name (DEEPSEEK) is replaced there.
      await api.setSecret(p.keys[0]?.name ?? p.secret, key.trim());
      key = "";
      editing = "";
      adding = false;
      await load();
      test(p);
      // Other machines get the key with the engines.
      quiet(api.shareEngines(), "share engines");
      onchange?.();
    } catch (e) {
      hub.fail(e);
    } finally {
      saving = false;
    }
  }

  function cancel() {
    editing = "";
    adding = false;
    key = "";
    query = "";
  }

  function openAdd() {
    adding = true;
    editing = "";
    key = "";
    query = "";
    queueMicrotask(() => search?.focus());
  }
</script>

{#snippet keyField(p: Provider)}
  <div class="row">
    <input
      class="field mono"
      type="password"
      placeholder={`Paste ${p.label}'s key`}
      bind:value={key}
      spellcheck="false"
      autocomplete="off"
      onkeydown={(e) => e.key === "Enter" && save()}
    />
    <button class="btn primary" disabled={saving || !key.trim()} onclick={save}>{saving ? "Saving…" : "Save and try"}</button>
    <button class="btn" onclick={cancel}>Cancel</button>
    {#if p.keys_at}<button class="link" onclick={() => openUrl(p.keys_at!)}>Get a key</button>{/if}
  </div>
  <div class="hint">Paid per token to {p.label}.</div>
{/snippet}

{#if failed}
  <div class="empty">Could not read the providers: {failed}</div>
{:else if !catalog}
  <Quiet kind="loading" />
{:else}
  <div class="rack">
    {#each mine as p (p.id)}
      {@const t = tests[p.id]}
      <div class="rack-row">
        <div class="rack-face">
          <span class="rack-badge"><ProviderLogo id={p.id} label={p.label} size={18} /></span>
          <span class="rack-main">
            <span class="rack-name">
              {p.label}
              {#if t === "running"}<span class="pill">Trying…</span>
              {:else if t?.ok}<span class="pill ok" data-tip={t.models.length ? `${t.models.length} models: ${t.models.slice(0, 8).join(", ")}${t.models.length > 8 ? " …" : ""}` : t.message}>Works</span>
              {:else if t}<span class="pill bad">Not working</span>{/if}
            </span>
            <span class="rack-sub">{p.keys[0].name} {p.keys[0].hint} · in {usedBy(p).join(", ")}</span>
          </span>
          <span class="rack-end">
            <button class="btn" disabled={t === "running"} onclick={() => test(p)}>Try</button>
            {#if editing !== p.id}<button class="btn" onclick={() => { editing = p.id; adding = false; key = ""; }}>Replace key</button>{/if}
          </span>
        </div>
        <!-- A working key needs no more words than its stamp; only a failure says what went wrong. -->
        {#if (t && t !== "running" && !t.ok) || (editing === p.id && !adding)}
          <div class="rack-more">
            {#if t && t !== "running" && !t.ok}<span class="bad">{t.message}</span>{/if}
            {#if editing === p.id && !adding}{@render keyField(p)}{/if}
          </div>
        {/if}
      </div>
    {:else}
      <div class="rack-row"><div class="rack-more first">No keys.</div></div>
    {/each}
    {@render after?.()}
    {#if !adding && others.length}
      <button class="rack-add" onclick={openAdd}><Icon name="plus" size={12} /> Add provider</button>
    {/if}
  </div>

  {#if adding}
    <div class="add" role="dialog" aria-label="Add a provider">
      {#if !picked}
        <div class="add-head">
          <span class="add-title">Add a provider</span>
          <div class="search">
            <Icon name="search" size={13} />
            <input bind:this={search} bind:value={query} placeholder="Search providers and models" spellcheck="false" onkeydown={(e) => e.key === "Escape" && cancel()} />
          </div>
          <button class="btn" onclick={cancel}>Cancel</button>
        </div>
        <div class="grid">
          {#each shown as p (p.id)}
            <button class="card" onclick={() => { editing = p.id; key = ""; }}>
              <div class="card-top"><ProviderLogo id={p.id} label={p.label} size={26} /><span class="card-name">{p.label}</span></div>
              <div class="card-what">{p.what}</div>
              <div class="card-foot">{p.models.slice(0, 3).join(" · ")}</div>
            </button>
          {:else}
            <div class="empty">No provider matches "{query}".</div>
          {/each}
        </div>
      {:else}
        <div class="add-head">
          <button class="back" onclick={() => { editing = ""; key = ""; }} data-tip="Back to the list"><Icon name="back" size={14} /></button>
          <ProviderLogo id={picked.id} label={picked.label} size={28} />
          <div class="picked">
            <span class="add-title">{picked.label}</span>
            <span class="hint">{picked.what} Runs in {usedBy(picked).join(", ")}.</span>
          </div>
        </div>
        {@render keyField(picked)}
      {/if}
    </div>
  {/if}
{/if}

<style>
  .hint { color: var(--muted); font-size: 13px; }
  .pill { font-size: 12px; color: var(--text-3); background: var(--surface-hover); border-radius: var(--r-sm); padding: 1px 8px; white-space: nowrap; }
  .pill.ok { color: var(--green); background: var(--green-bg); }
  .pill.bad { color: var(--red); background: var(--red-bg); }
  .rack-more.first { padding: 10px 14px 10px 62px; color: var(--muted); }
  .bad { color: var(--red); }
  .add { display: flex; flex-direction: column; gap: 12px; padding: 14px; margin-top: 10px; border: 1px solid var(--line-strong); border-radius: var(--r-lg); background: var(--surface); }
  .add-head { display: flex; align-items: center; gap: 12px; }
  .add-title { color: var(--text); font-size: 14px; }
  .picked { display: flex; flex-direction: column; gap: 2px; min-width: 0; }
  .search { flex: 1; display: flex; align-items: center; gap: 6px; height: 28px; padding: 0 8px; border: 1px solid var(--line-strong); border-radius: var(--r-md); background: var(--bg); color: var(--muted); max-width: 360px; margin-left: auto; }
  .search:focus-within { border-color: var(--accent); }
  .search input { flex: 1; background: none; border: none; outline: none; color: var(--text); font-size: 13px; }
  .grid { display: grid; grid-template-columns: repeat(auto-fill, minmax(220px, 1fr)); gap: 8px; }
  .card { display: flex; flex-direction: column; gap: 6px; padding: 12px; border: 1px solid var(--line); border-radius: var(--r-md); background: var(--bg); text-align: left; min-height: 104px; }
  .card:hover, .card:focus-visible { border-color: var(--line-strong); background: var(--surface-hover); }
  .card-top { display: flex; align-items: center; gap: 10px; }
  .card-name { color: var(--text); font-size: 14px; }
  .card-what { color: var(--text-3); font-size: 13px; line-height: 1.4; }
  .card-foot { margin-top: auto; color: var(--muted); font-size: 12px; font-family: var(--mono, monospace); overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .back { width: 26px; height: 26px; display: inline-flex; align-items: center; justify-content: center; border-radius: var(--r-sm); color: var(--text-3); }
  .back:hover { background: var(--surface-hover); color: var(--text); }
  .row { display: flex; gap: 8px; align-items: center; margin-top: 4px; }
  .field { height: 28px; padding: 0 8px; background: var(--bg); border: 1px solid var(--line-strong); border-radius: var(--r-md); font-size: 13px; outline: none; color: var(--text); width: 100%; max-width: 420px; }
  .field:focus { border-color: var(--accent); }
  .link { color: var(--accent); font-size: 13px; padding: 0; white-space: nowrap; }
  .empty { color: var(--muted); padding: 12px 0; font-size: 13px; }
</style>
