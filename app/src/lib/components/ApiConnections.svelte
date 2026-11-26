<!-- Settings > API connections: outside services Hub reaches with a key you add by hand, such as Jev. The page lists your connections and how each is doing, and adds new ones; a connection opens on its own page (`hub.apiOpen`) with its key, use, projects and the rest. Keys go to the vault; engines never get them.
     Used by Settings.svelte. -->
<script lang="ts">
  import { autofocus } from "../autofocus";
  import { quiet } from "../quiet";
  import { poll } from "../poll";
  import Quiet from "./Quiet.svelte";
  // Settings > API connections: outside services Hub reaches with a key you add by hand, such as
  // Jev. The page lists your connections and how each is doing, and adds new ones; a connection
  // opens on its own page (`hub.apiOpen`) with its key, use, projects and the rest. Keys go to the
  // vault; engines never get them.
  import { onDestroy, onMount } from "svelte";
  import { api } from "../api";
  import { apis, HEALTH, type Health } from "../apis.svelte";
  import type { ApiAnswer } from "../bindings/ApiAnswer";
  import type { ApiAuth } from "../bindings/ApiAuth";
  import type { ApiConnection } from "../bindings/ApiConnection";
  import type { ApiDecision } from "../bindings/ApiDecision";
  import type { ApiKind } from "../bindings/ApiKind";
  import type { ApiPrefs } from "../bindings/ApiPrefs";
  import type { ApiTest } from "../bindings/ApiTest";
  import type { TuneSuggestion } from "../bindings/TuneSuggestion";
  import { ago, folderName } from "../format";
  import { openUrl } from "../native";
  import { hub } from "../state.svelte";
  import Icon from "./Icon.svelte";
  import ProviderLogo from "./ProviderLogo.svelte";
  import { BRANDS } from "../providerLogos";
  import ProviderList from "./ProviderList.svelte";
  import { reloadEngines } from "../providers.svelte";
  import Switch from "./Switch.svelte";
  import Select from "./Select.svelte";
  import ApiUse from "./ApiUse.svelte";
  import { CONTRIB, LOOKS, brandStyle, envName, host, says, apiSlug, tokens } from "../apiConnections";

  /** A check older than this is run again when the list opens. */
  const STALE_MS = 6 * 3_600_000;

  let decisions = $state<ApiDecision[]>([]);
  let keyDraft = $state("");
  let testing = $state<Record<string, boolean>>({});
  let tested = $state<Record<string, ApiTest>>({});
  /** The name typed in Add a connection. */
  let query = $state("");
  /** A connection being added: a known service (`kind`) or any other (`kind` null). */
  let adding = $state<{
    kind: ApiKind | null; label: string; url: string; paste: string; test_path: string;
    /** Saved under this name; for a login, waiting for Allow. */
    name: string | null; waiting: boolean; busy: boolean; error: string;
  } | null>(null);
  let copied = $state(false);
  /** The card grid of services to add is open. */
  let picking = $state(false);
  let searchBox = $state<HTMLInputElement>();
  let polling: (() => void) | null = null;
  onDestroy(() => stopPolling());

  const data = $derived(apis.data);
  const open = $derived(data?.prefs.connections.find((c) => c.name === hub.apiOpen) ?? null);

  async function load() {
    try {
      const [state, d] = await Promise.all([apis.load(), api.apiDecisions()]);
      decisions = d;
      return state;
    } catch (e) {
      hub.fail(e);
    }
  }

  onMount(async () => {
    quiet(apis.loadTune(), "load tune");
    const state = await load();
    // Keep "working" honest without a button press: check keys not checked lately.
    for (const c of state?.prefs.connections ?? []) {
      const s = state!.status.find((x) => x.name === c.name);
      if (s?.key && (!s.check || Date.now() - s.check.at > STALE_MS)) test(c);
    }
  });

  // A new page for each connection: what was typed for the last one doesn't carry over.
  $effect(() => {
    void hub.apiOpen;
    keyDraft = "";
  });

  async function save(change: (p: ApiPrefs) => void) {
    if (!data) return;
    const prefs = $state.snapshot(data.prefs);
    change(prefs);
    try {
      await api.setApis(prefs);
      await load();
    } catch (e) {
      hub.fail(e);
      throw e;
    }
  }

  const kindOf = (c: ApiConnection) => data?.catalog.find((k) => k.kind === c.kind);
  const asks = (c: ApiConnection) => !!kindOf(c)?.asks;
  const projectName = (path: string) => hub.projects.find((p) => p.path.toLowerCase() === path.toLowerCase())?.name || folderName(path);
  const notAdded = $derived(data?.catalog.filter((k) => !data.prefs.connections.some((c) => c.kind === k.kind)) ?? []);

  function healthLine(c: ApiConnection): string {
    const s = apis.status(c);
    if (testing[c.name]) return "Checking…";
    const h = apis.health(c);
    if (h === "nokey") return isLogin(c.auth) ? "Press Allow to use it" : "Add its key to use it";
    if (h === "unchecked") return "Not checked yet";
    return `${HEALTH[h]} · checked ${ago(s!.check!.at)}`;
  }

  const isLogin = (a: ApiAuth | undefined) => a?.type === "login";

  /** The known services the typed name points at, or all of them before anything is typed. */
  const matches = $derived.by(() => {
    const q = query.trim().toLowerCase();
    return q ? notAdded.filter((k) => k.label.toLowerCase().includes(q) || k.kind.includes(q)) : notAdded;
  });

  function openPicker() {
    picking = true;
    query = "";
    queueMicrotask(() => searchBox?.focus());
  }

  function startAdd(kind: ApiKind | null, label = query.trim()) {
    copied = false;
    picking = false;
    adding = {
      kind, label: kind?.label ?? label, url: kind ? kind.url : "https://", paste: "",
      test_path: kind?.test_path ?? "", name: null, waiting: false, busy: false, error: "",
    };
    query = "";
  }

  function onQueryKey(e: KeyboardEvent) {
    if (e.key !== "Enter" || !query.trim()) return;
    const q = query.trim().toLowerCase();
    startAdd(notAdded.find((k) => k.label.toLowerCase() === q || k.kind === q) ?? (matches.length === 1 ? matches[0] : null));
  }

  /** The address a service that logs you in sends you back to, for its Redirect URIs. */
  async function copyRedirect() {
    if (data) await quiet(navigator.clipboard.writeText(data.redirect).then(() => (copied = true)), "write text");
  }

  /** Opens where the key is made. For a login, the address the service asks for is copied first. */
  async function openSetup(k: ApiKind) {
    if (isLogin(k.auth)) await copyRedirect();
    if (k.keys_at) openUrl(k.keys_at);
  }

  function freeName(label: string): string {
    const taken = new Set(data?.prefs.connections.map((c) => c.name));
    const base = apiSlug(label);
    let name = base;
    for (let n = 2; taken.has(name); n++) name = `${base}-${n}`;
    return name;
  }

  function stopPolling() {
    if (polling) polling();
    polling = null;
  }

  /** Saves the connection and its key, then tests it; a login opens the service's Allow page. */
  async function connect() {
    const a = adding;
    if (!a || a.busy || !a.paste.trim()) return;
    a.busy = true;
    a.error = "";
    const k = a.kind;
    const pasted = a.paste.trim();
    try {
      const name = a.name ?? freeName(a.label);
      const login = isLogin(k?.auth);
      const secret = k ? k.secret : `${name.toUpperCase().replace(/-/g, "_")}_API_KEY`;
      if (!a.name) {
        await save((p) => {
          p.connections.push({
            name, kind: k?.kind ?? "custom", label: a.label.trim(), url: a.url.trim(), secret, model: k?.model ?? null,
            test_path: a.test_path.trim(), docs: k?.docs ?? null, daily_tokens: 0, projects: [],
            auth: k?.auth ?? { type: "bearer" }, client_id: login ? pasted : null,
          });
        });
        a.name = name;
      } else if (login) {
        await save((p) => (p.connections.find((x) => x.name === name)!.client_id = pasted));
      }
      if (login) {
        await allow(name);
        a.waiting = true;
        stopPolling();
        const started = Date.now();
        polling = poll(async () => {
          const state = await quiet(apis.load(), "load", null);
          const s = state?.status.find((x) => x.name === name);
          if (s?.key || Date.now() - started > 15 * 60_000) {
            stopPolling();
            if (s?.key) {
              adding = null;
              hub.apiOpen = name;
            }
          }
        }, 2000);
      } else {
        await api.setSecret(secret, pasted);
        const c = (await load())?.prefs.connections.find((x) => x.name === name);
        if (c) await test(c);
        adding = null;
        hub.apiOpen = name;
      }
    } catch (e) {
      a.error = e instanceof Error ? e.message : String(e);
    } finally {
      a.busy = false;
    }
  }

  /** Opens the service's Allow page for a connection that logs you in. */
  async function allow(name: string) {
    const { url } = await api.apiLogin(name);
    openUrl(url);
  }

  function cancelAdd() {
    stopPolling();
    adding = null;
    picking = false;
  }

  /** Back from a service's steps to the cards. */
  function backToCards() {
    stopPolling();
    adding = null;
    openPicker();
  }



  async function remove(c: ApiConnection) {
    await save((p) => {
      p.connections = p.connections.filter((x) => x.name !== c.name);
      if (p.helper === c.name) p.helper = null;
    });
    if (data && !data.prefs.connections.some((x) => x.secret === c.secret)) {
      await api.deleteSecret(c.secret).catch(hub.fail);
      await load();
    }
    hub.apiOpen = null;
  }

  async function saveKey(c: ApiConnection) {
    const value = keyDraft.trim();
    if (!value) return;
    try {
      await api.setSecret(c.secret, value);
      keyDraft = "";
      await load();
      await test(c);
    } catch (e) {
      hub.fail(e);
    }
  }

  async function test(c: ApiConnection) {
    testing[c.name] = true;
    try {
      tested[c.name] = await api.testApi(c.name);
    } catch (e) {
      tested[c.name] = { ok: false, message: e instanceof Error ? e.message : String(e), ms: 0, models: [] };
    }
    testing[c.name] = false;
    await load();
  }

  function toggleProject(c: ApiConnection, path: string, on: boolean) {
    save((p) => {
      const x = p.connections.find((y) => y.name === c.name)!;
      x.projects = on ? [...x.projects, path] : x.projects.filter((q) => q.toLowerCase() !== path.toLowerCase());
    });
  }

</script>

{#snippet dot(h: Health)}
  <span class="dot {h}"></span>
{/snippet}

{#if open}
  {@const c = open}
  {@const s = apis.status(c)}
  {@const k = kindOf(c)}
  {@const h = apis.health(c)}
  {@const t = tested[c.name]}
  {@const look = LOOKS[c.kind]}
  <div class="branded" style={brandStyle(c.kind)}>
  <button class="back" onclick={() => (hub.apiOpen = null)}><Icon name="chevron" /> API connections</button>
  <header class="hero look-{look ? c.kind : 'plain'}">
    {#if c.kind === "github"}
      <div class="deco grid-deco" aria-hidden="true">{#each CONTRIB as level, i (i)}<span class="sq l{level}"></span>{/each}</div>
    {:else if c.kind === "spotify" || c.kind === "openweather"}
      <div class="deco mark-deco" aria-hidden="true"><ProviderLogo id={c.kind} size={190} color={c.kind === "spotify" ? "#000" : "#fff"} /></div>
    {/if}
    <span class="tile"><ProviderLogo id={c.kind} label={c.label} size={30} color={BRANDS[c.kind]} /></span>
    <div class="hero-text">
      {#if look?.kicker}<div class="kicker">{look.kicker}</div>{/if}
      <div class="title">
        <h1>{c.label}</h1>
        <span class="state {h}">{@render dot(h)}{testing[c.name] ? "Checking…" : HEALTH[h]}</span>
      </div>
      <p class="lead">{k?.what ?? `An API you added. Hub sends its key ${says(c.auth)}.`} <span class="mono dim">{host(c.url)}</span></p>
    </div>
    <div class="hero-actions">
      {#if c.docs}<button class="btn small" onclick={() => openUrl(c.docs!)}>Docs</button>{/if}
      <button class="btn small danger" onclick={() => remove(c)}>Remove</button>
    </div>
  </header>

  <h2>{isLogin(c.auth) ? "Login" : "Key"}</h2>
  <section>
    {#if isLogin(c.auth)}
      <div class="line">
        <div class="what">
          <div class="name">{s?.key ? "Logged in" : "Not allowed yet"}</div>
          <div class="hint">
            {#if s?.key}Renewed by itself. Press Allow again if {c.label} drops it.
            {:else}Press Allow, then Allow on {c.label}'s page.{/if}
            Client ID <span class="mono">…{(c.client_id ?? "").slice(-4)}</span>.
          </div>
        </div>
        <div class="control"><button class="btn small" class:primary={!s?.key} onclick={() => allow(c.name).catch(hub.fail)}>{s?.key ? "Allow again" : "Allow"}</button></div>
      </div>
    {:else}
    <div class="line">
      <div class="what">
        <div class="name mono">{c.secret}</div>
        <div class="hint">
          {#if s?.key}Ends in <span class="mono">{s.key}</span>. See the <button class="link" onclick={() => { hub.keyOpen = c.secret; hub.settingsSection = "vault"; }}>Key vault</button>.
          {:else}No key yet.{#if k?.keys_at} Make one at <button class="link" onclick={() => openUrl(k.keys_at!)}>{host(k.keys_at)}</button>.{/if}{/if}
        </div>
      </div>
      <div class="control">
        <input class="field key" type="password" placeholder={s?.key ? "New key" : "Paste the key"} autocomplete="off" spellcheck="false"
          bind:value={keyDraft} onkeydown={(e) => e.key === "Enter" && saveKey(c)} />
        <button class="btn small primary" disabled={!keyDraft.trim()} onclick={() => saveKey(c)}>Save</button>
      </div>
    </div>
    {/if}
    <div class="line">
      <div class="what">
        <div class="name">Status</div>
        <div class="hint">
          {#if testing[c.name]}Checking the key…
          {:else if s?.check}{s.check.message}{" "}<span class="dim">checked {ago(s.check.at)}{t?.ms ? ` · ${t.ms} ms` : ""}</span>
          {:else}Not checked.{/if}
          {#if t?.models.length}<br />Models: <span class="mono">{t.models.join(", ")}</span>{/if}
        </div>
      </div>
      <div class="control"><button class="btn small" disabled={!s?.key || testing[c.name]} onclick={() => test(c)}>Test</button></div>
    </div>
  </section>

  <h2>Use</h2>
  <section>
    <div class="line">
      <div class="what">
        <div class="name">Today {s?.today.calls ?? 0} calls, {tokens((s?.today.input_tokens ?? 0) + (s?.today.output_tokens ?? 0))} tokens</div>
        <div class="hint">This month {s?.month.calls ?? 0} calls, {tokens((s?.month.input_tokens ?? 0) + (s?.month.output_tokens ?? 0))} tokens{#if s?.month.errors} · {s.month.errors} failed{/if}</div>
      </div>
      <div class="control">
        <span class="dim small-label">Daily cap</span>
        <Select class="cap" value={c.daily_tokens} label="Daily cap"
          onchange={(n) => save((p) => (p.connections.find((x) => x.name === c.name)!.daily_tokens = n))}
          options={[0, 100_000, 500_000, 2_000_000].map((n) => ({ value: n, label: n ? `${tokens(n)} tokens` : "None" }))} />
      </div>
    </div>
  </section>

  <h2>Projects</h2>
  {#if c.auth.type === "bearer"}
    <p class="sub">Projects that are on get <span class="mono">{c.secret}</span> and its address as environment variables.</p>
  {:else}
    <p class="sub">Projects that are on get <span class="mono">{envName(c.name)}</span>, an address that signs each call. No key is shown.</p>
  {/if}
  <section>
    {#each hub.projects as p (p.path)}
      {@const on = c.projects.some((q) => q.toLowerCase() === p.path.toLowerCase())}
      <div class="line slim">
        <div class="what"><div class="name">{p.name || folderName(p.path)}</div><div class="hint mono">{p.path}</div></div>
        <div class="control"><Switch label="Use {c.label} in {p.name}" {on} onchange={(v) => toggleProject(c, p.path, v)} /></div>
      </div>
    {:else}
      <Quiet text="No projects" />
    {/each}
  </section>

  {#if asks(c)}
    <ApiUse {c} {s} {decisions} {save} reload={load} />
  {/if}
  </div>
{:else}
  <h1>API connections</h1>
  <p class="lead">
    Services Hub reaches with a key.
  </p>

  <h2>Your connections</h2>
  <section class="list">
    {#each data?.prefs.connections ?? [] as c (c.name)}
      {@const h = testing[c.name] ? "unchecked" : apis.health(c)}
      {@const s = apis.status(c)}
      <button class="conn" onclick={() => (hub.apiOpen = c.name)}>
        <ProviderLogo id={c.kind} label={c.label} size={22} color={BRANDS[c.kind]} />
        <span class="who">
          <span class="name">{c.label} {@render dot(h)}</span>
          <span class="hint">{healthLine(c)}</span>
        </span>
        <span class="facts">
          <span>{c.projects.length ? c.projects.map(projectName).join(", ") : "No projects"}</span>
          <span>{s?.today.calls ? `${s.today.calls} calls today` : "No calls today"}</span>
        </span>
        <Icon name="chevron" />
      </button>
    {:else}
      {#if data}<Quiet text="None" />{:else}<Quiet kind="loading" />{/if}
    {/each}
  </section>
  {#if adding}
    {@const a = adding}
    {@const k = a.kind}
    {@const login = isLogin(k?.auth)}
    <div class="add adding" role="dialog" aria-label="Add {a.label}" style={brandStyle(k?.kind ?? "")}>
      <div class="add-head">
        <button class="icon-btn" onclick={backToCards} data-tip="Back to the list"><Icon name="back" size={14} /></button>
        <ProviderLogo id={k?.kind ?? "custom"} label={a.label} size={28} color={k ? BRANDS[k.kind] : ""} />
        <div class="picked">
          <span class="add-title">{a.label}</span>
          <span class="hint">{k?.what ?? "Give it the address and the key."}</span>
        </div>
        <button class="btn" onclick={cancelAdd}>Cancel</button>
      </div>
      {#if k}
        <div class="step">
          <span class="n" class:done={copied || !!a.name}>1</span>
          <div class="what">
            <div class="name">{login ? `Make an app on ${k.label}` : `Get ${k.paste === "Token" ? "a token" : `an ${k.paste}`} from ${k.label}`}</div>
            <div class="hint">{k.setup}</div>
            {#if login}
              <div class="redirect">
                <span class="mono">{data?.redirect}</span>
                <button class="btn small" onclick={copyRedirect}>{copied ? "Copied" : "Copy"}</button>
              </div>
            {/if}
          </div>
          {#if k.keys_at}<div class="control"><button class="btn small" onclick={() => openSetup(k)}>Open {k.label} <Icon name="external" /></button></div>{/if}
        </div>
      {:else}
        <div class="step">
          <span class="n">1</span>
          <div class="what">
            <div class="name">Its address</div>
            <input class="field" bind:value={a.url} placeholder="https://api.example.com" spellcheck="false" />
            <input class="field" bind:value={a.test_path} placeholder="A path that needs the key, to test with (optional), e.g. /v1/me" spellcheck="false" />
          </div>
        </div>
      {/if}
      <div class="step">
        <span class="n" class:done={a.waiting}>2</span>
        <div class="what">
          <div class="name">Paste {k ? `its ${k.paste}` : "its key"}</div>
          <input class="field" type={login ? "text" : "password"} autocomplete="off" spellcheck="false" use:autofocus={!!k}
            bind:value={a.paste} placeholder={k?.paste ?? "Key or token"} onkeydown={(e) => e.key === "Enter" && connect()} />
          
        </div>
        <div class="control">
          <button class="btn small primary" disabled={!a.paste.trim() || a.busy || (!k && !a.url.trim().match(/^https?:\/\/./))} onclick={connect}>
            {a.busy ? "Connecting…" : a.waiting ? "Open again" : "Connect"}
          </button>
        </div>
      </div>
      {#if login}
        <div class="step">
          <span class="n">3</span>
          <div class="what">
            <div class="name">Allow Hub on {k?.label}'s page</div>
            <div class="hint">{a.waiting ? `Waiting for Allow in the browser…` : "Opens by itself."}</div>
            {#if a.waiting}
              <div class="hint">
                “redirect_uri: Not matching configuration”? Save the address from step 1 under Redirect URIs, then press Open again.
              </div>
            {/if}
          </div>
        </div>
      {/if}
      {#if a.error}<div class="error">{a.error}</div>{/if}
    </div>
  {:else if picking}
    <div class="add" role="dialog" aria-label="Add a connection">
      <div class="add-head">
        <span class="add-title">Add a connection</span>
        <div class="search">
          <Icon name="search" size={13} />
          <input bind:this={searchBox} bind:value={query} onkeydown={(e) => { if (e.key === "Escape") cancelAdd(); else onQueryKey(e); }}
            spellcheck="false" placeholder="Name a service, like Spotify, or any other" />
        </div>
        <button class="btn" onclick={cancelAdd}>Cancel</button>
      </div>
      <div class="grid">
        {#each matches as k (k.kind)}
          <button class="card" onclick={() => startAdd(k)}>
            <div class="card-top"><ProviderLogo id={k.kind} label={k.label} size={26} color={BRANDS[k.kind]} /><span class="card-name">{k.label}</span></div>
            <div class="card-what">{k.what}</div>
            <div class="card-foot">{isLogin(k.auth) ? "Log in once" : `Paste ${k.paste === "Token" ? "a token" : `an ${k.paste}`}`} · {host(k.url)}</div>
          </button>
        {/each}
        {#if !matches.some((k) => k.label.toLowerCase() === query.trim().toLowerCase())}
          <button class="card" onclick={() => startAdd(null)}>
            <div class="card-top"><span class="plus"><Icon name="plus" size={14} /></span><span class="card-name">{query.trim() ? `Add “${query.trim()}”` : "Another API"}</span></div>
            <div class="card-what">Any service with an address and a key. Hub tries the key and works out how the service wants it.</div>
            <div class="card-foot">Paste a key · any address</div>
          </button>
        {/if}
      </div>
    </div>
  {:else}
    <div class="line"><button class="btn" onclick={openPicker}>Add connection</button></div>
  {/if}

  <h2>Model providers</h2>
  <p class="hint">Same list as Engines &gt; Providers.</p>
  <section>
    <ProviderList onchange={() => reloadEngines()} />
  </section>

{/if}

<style>
  .more { margin-top: 8px; }
  h1 { color: var(--text); font-size: 20px; font-weight: 600; margin: 0; }
  h2 { color: var(--text); font-size: 15px; font-weight: 600; margin: 30px 0 4px; }
  .lead { color: var(--text-3); margin: 8px 0 8px; font-size: 14px; }
  .sub { color: var(--muted); font-size: 13px; margin: 0 0 4px; }
  .back { display: inline-flex; align-items: center; gap: 4px; color: var(--muted); font-size: 13px; margin-bottom: 10px; }
  .back:hover { color: var(--text); }
  .back :global(svg) { transform: rotate(180deg); width: 14px; height: 14px; }
  .title { display: flex; align-items: center; gap: 12px; }
  .grow { flex: 1; }
  .state { display: inline-flex; align-items: center; gap: 6px; font-size: 13px; color: var(--text-3); background: var(--surface-hover); border-radius: var(--r-sm); padding: 2px 8px; }
  .state.working { color: var(--green); background: var(--green-bg); }
  .state.failing { color: var(--red); background: var(--red-bg); }
  .dot { width: 7px; height: 7px; border-radius: 50%; background: var(--line-strong); flex: none; }
  .dot.working { background: var(--green); }
  .dot.failing { background: var(--red); }
  .dot.nokey { background: transparent; border: 1px solid var(--muted); }
  .line { display: flex; align-items: center; gap: 32px; padding: 14px 0; }
  .line + .line { border-top: 1px solid var(--line); }
  .line.slim { padding: 9px 0; }
  .line.top { align-items: flex-start; }
  .what { flex: 1; min-width: 0; display: flex; flex-direction: column; gap: 4px; }
  .name { color: var(--text); font-size: 14px; }
  .hint { color: var(--muted); font-size: 13px; }
  .control { display: flex; align-items: center; gap: 8px; flex: none; }
  /* As wide as its longest choice, so picking one does not move the row. */
  .control :global(.select.cap) { min-width: 150px; }
  .dim { color: var(--muted); }
  .small-label { font-size: 12px; }
  .field { width: 100%; height: 28px; padding: 0 9px; background: var(--bg); border: 1px solid var(--line-strong); border-radius: var(--r-md); font-size: 13px; outline: none; color: var(--text); }
  .field:focus { border-color: var(--accent); }
  .field.key { width: 220px; }
  .field.area { height: auto; padding: 6px 9px; resize: vertical; font-family: inherit; }
  .form { gap: 6px; }
  .link { color: var(--accent); font-size: inherit; padding: 0; }
  .conn { display: flex; align-items: center; gap: 12px; width: 100%; padding: 11px 10px; border-radius: var(--r-md); text-align: left; color: var(--muted); }
  .conn:hover { background: var(--surface); }
  .conn + .conn { border-top: 1px solid var(--line); }
  .conn .who { flex: 1; min-width: 0; display: flex; flex-direction: column; gap: 2px; }
  .conn .name { display: inline-flex; align-items: center; gap: 7px; }
  .conn .facts { display: flex; flex-direction: column; align-items: flex-end; gap: 2px; font-size: 12px; color: var(--muted); max-width: 40%; text-align: right; }
  .conn :global(svg) { width: 14px; height: 14px; flex: none; }
  .when { color: var(--muted); font-size: 12px; flex: none; }
  .pill { font-size: 12px; color: var(--text-3); background: var(--surface-hover); border-radius: var(--r-sm); padding: 1px 7px; white-space: nowrap; }
  .pill.warn { color: var(--amber); background: var(--amber-bg); }
  .pill.dim { background: transparent; border: 1px solid var(--line); }
  .intro { color: var(--text-3); font-size: 13px; line-height: 1.5; margin: 2px 0 6px; }
  .btn.small { height: 24px; padding: 0 9px; font-size: 12px; }
  .err { color: var(--red); }
  .empty { color: var(--muted); padding: 12px 0; font-size: 13px; }
  .add { display: flex; flex-direction: column; gap: 12px; padding: 14px; margin-top: 6px; border: 1px solid var(--line-strong); border-radius: var(--r-lg); background: var(--surface); }
  .add.adding { gap: 0; }
  .add-head { display: flex; align-items: center; gap: 12px; }
  .add.adding .add-head { padding-bottom: 6px; }
  .add-title { color: var(--text); font-size: 14px; }
  .picked { display: flex; flex-direction: column; gap: 2px; min-width: 0; flex: 1; }
  .icon-btn { width: 26px; height: 26px; display: inline-flex; align-items: center; justify-content: center; border-radius: var(--r-md); color: var(--muted); flex: none; }
  .icon-btn:hover { background: var(--surface-hover); color: var(--text); }
  .search { flex: 1; display: flex; align-items: center; gap: 6px; height: 28px; padding: 0 9px; border: 1px solid var(--line-strong); border-radius: var(--r-md); background: var(--bg); color: var(--muted); max-width: 360px; margin-left: auto; }
  .search:focus-within { border-color: var(--accent); }
  .search input { flex: 1; background: none; border: none; outline: none; color: var(--text); font-size: 13px; }
  .grid { display: grid; grid-template-columns: repeat(auto-fill, minmax(220px, 1fr)); gap: 8px; }
  .card { display: flex; flex-direction: column; gap: 6px; padding: 12px; border: 1px solid var(--line); border-radius: var(--r-md); background: var(--bg); text-align: left; min-height: 104px; }
  .card:hover, .card:focus-visible { border-color: var(--line-strong); background: var(--surface-hover); }
  .card-top { display: flex; align-items: center; gap: 10px; }
  .card-name { color: var(--text); font-size: 14px; }
  .card-what { color: var(--text-3); font-size: 13px; line-height: 1.4; }
  .card-foot { margin-top: auto; color: var(--muted); font-size: 12px; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .plus { width: 26px; height: 26px; border-radius: var(--r-sm); border: 1px dashed var(--line-strong); display: inline-flex; align-items: center; justify-content: center; color: var(--muted); flex: none; }
  /* A connection's page in its own colour (BRANDS): the header is tinted with it, and Hub's accent
     takes it, so buttons, links and switches on the page follow. */
  .hero { position: relative; overflow: hidden; display: flex; align-items: flex-start; gap: 16px; padding: 18px; border-radius: var(--r-lg);
    border: 1px solid color-mix(in srgb, var(--brand) 28%, var(--line));
    background: color-mix(in srgb, var(--brand) 7%, var(--surface)); }
  .hero > :not(.deco) { position: relative; }
  .deco { position: absolute; pointer-events: none; }
  .kicker { font-size: 12px; margin-bottom: 6px; color: var(--muted); }

  /* Each known service's header is a small copy of its own site (LOOKS). */
  /* TypeSafe: dithered pink, blue and purple clouds, huge tight black type, white boxes in mono. */
  .look-typesafe { border: none; border-radius: 4px; padding: 26px 26px 30px;
    background:
      url("data:image/svg+xml;utf8,<svg xmlns='http://www.w3.org/2000/svg' width='140' height='140'><filter id='n'><feTurbulence type='fractalNoise' baseFrequency='1.4' numOctaves='2' stitchTiles='stitch'/><feColorMatrix values='0 0 0 0 1  0 0 0 0 1  0 0 0 0 1  0 0 0 1.6 -0.55'/></filter><rect width='100%' height='100%' filter='url(%23n)'/></svg>"),
      radial-gradient(ellipse 38% 70% at 6% 40%, #ff4fc8 0%, transparent 70%),
      radial-gradient(ellipse 35% 75% at 96% 35%, #ff55d0 0%, transparent 70%),
      radial-gradient(ellipse 30% 60% at 80% 70%, #9a6cff 0%, transparent 75%),
      radial-gradient(ellipse 60% 55% at 50% 110%, #ffffff 0%, transparent 70%),
      linear-gradient(180deg, #6fa9cf 0%, #8fc3dc 55%, #dfeef5 100%);
    image-rendering: pixelated; }
  .look-typesafe .kicker { font-family: var(--mono, ui-monospace, monospace); color: #1e1e1e; font-size: 11px; }
  .look-typesafe .title h1 { color: #1e1e1e; font-size: 40px; font-weight: 600; letter-spacing: -0.045em; line-height: 0.95; }
  .look-typesafe .lead { color: #1e1e1e; font-family: var(--mono, ui-monospace, monospace); font-size: 12px; max-width: 70ch; }
  .look-typesafe .lead .dim { color: #3b3b3b; }
  .look-typesafe .tile { background: #fff; border: none; border-radius: 0; }
  .look-typesafe .btn { background: #fff; color: #1e1e1e; border-radius: 0; }
  .look-typesafe .btn.danger { background: #1e1e1e; color: #fff; }
  .look-typesafe .state { border-radius: 0; background: #fff; }

  /* Spotify: its green fading into black, white bold type, pill buttons, the mark cut off at the edge. */
  .look-spotify { border: none; padding: 24px; background: linear-gradient(180deg, #1ed760 -40%, #127a3a 25%, #121212 100%); }
  .look-spotify .title h1 { color: #fff; font-size: 34px; font-weight: 800; letter-spacing: -0.03em; }
  .look-spotify .lead { color: #d6d6d6; }
  .look-spotify .tile { background: #000; border: none; border-radius: 50%; box-shadow: 0 6px 20px rgba(0, 0, 0, 0.45); }
  .look-spotify .btn { border-radius: 999px; background: transparent; border: 1px solid #7c7c7c; color: #fff; font-weight: 600; }
  .look-spotify .btn:hover { border-color: #fff; }
  .look-spotify .state { border-radius: 999px; }
  .look-spotify .mark-deco { right: -40px; top: -30px; opacity: 0.18; }

  /* GitHub: its dark canvas, hairline borders, monospace, and a contribution graph. */
  .look-github { background: #0d1117; border-color: #30363d; border-radius: 6px; }
  .look-github .kicker { font-family: var(--mono, ui-monospace, monospace); color: #7d8590; }
  .look-github .title h1 { color: #e6edf3; font-weight: 600; }
  .look-github .lead { color: #9198a1; }
  .look-github .tile { background: #161b22; border-color: #30363d; border-radius: 50%; }
  .look-github .btn { background: #21262d; border: 1px solid #3d444d; color: #e6edf3; border-radius: 6px; }
  .look-github .grid-deco { right: 14px; bottom: 12px; display: grid; grid-template-rows: repeat(7, 9px); grid-auto-flow: column; grid-auto-columns: 9px; gap: 3px; opacity: 0.55; }
  .look-github .sq { border-radius: 2px; background: #161b22; outline: 1px solid rgba(255, 255, 255, 0.04); }
  .look-github .sq.l1 { background: #0e4429; }
  .look-github .sq.l2 { background: #006d32; }
  .look-github .sq.l3 { background: #26a641; }
  .look-github .sq.l4 { background: #39d353; }

  /* OpenWeather: its warm orange, white type, rounded, a big sun. */
  .look-openweather { border: none; border-radius: 16px; padding: 24px; background: linear-gradient(135deg, #eb6e4b 0%, #f08a5d 60%, #f5a76c 100%); }
  .look-openweather .kicker { color: rgba(255, 255, 255, 0.85); }
  .look-openweather .title h1 { color: #fff; font-size: 28px; }
  .look-openweather .lead, .look-openweather .lead .dim { color: rgba(255, 255, 255, 0.92); }
  .look-openweather .tile { background: #fff; border: none; border-radius: 14px; }
  .look-openweather .btn { background: rgba(255, 255, 255, 0.92); color: #b2462a; border-radius: 10px; }
  .look-openweather .mark-deco { right: -30px; top: -40px; opacity: 0.22; }
  .hero .title h1 { font-size: 22px; }
  .hero .lead { margin: 6px 0 0; }
  .hero-text { flex: 1; min-width: 0; }
  .hero-actions { display: flex; gap: 8px; flex: none; }
  .tile { width: 52px; height: 52px; border-radius: var(--r-lg); display: flex; align-items: center; justify-content: center; flex: none;
    background: color-mix(in srgb, var(--brand) 14%, var(--bg)); border: 1px solid color-mix(in srgb, var(--brand) 30%, transparent); }
  .branded h2 { display: flex; align-items: center; gap: 8px; }
  .branded h2::before { content: ""; width: 6px; height: 6px; border-radius: 2px; background: var(--brand); }
  .step { display: flex; align-items: flex-start; gap: 14px; padding: 12px 0; }
  .step .what { gap: 6px; }
  .n { width: 20px; height: 20px; border-radius: 50%; border: 1px solid var(--line-strong); color: var(--muted); font-size: 12px; display: flex; align-items: center; justify-content: center; flex: none; margin-top: 1px; }
  .n.done { border-color: var(--green); color: var(--green); }
  .btn :global(svg) { width: 12px; height: 12px; }
  .redirect { display: flex; align-items: center; gap: 10px; padding: 6px 10px; background: var(--surface); border: 1px solid var(--line); border-radius: var(--r-md); width: fit-content; }
  .redirect .mono { color: var(--text-2); font-size: 13px; user-select: all; }
  .error { color: var(--red); font-size: 13px; padding: 4px 0 8px 34px; }
</style>
