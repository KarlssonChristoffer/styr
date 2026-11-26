<!-- The MCP servers and connectors each engine's agents can reach, where each comes from, whether it answers, and which of its tools threads called. claude.ai connectors come with the Claude login by themselves, so this is where they stop being a surprise. Each can be switched off for Hub's threads only; a terminal keeps everything, and threads already running keep what they had.
     Used by Settings.svelte. -->
<script lang="ts">
  import { quiet } from "../quiet";
  import Quiet from "./Quiet.svelte";
  // The MCP servers and connectors each engine's agents can reach, where each comes from, whether
  // it answers, and which of its tools threads called. claude.ai connectors come with the Claude
  // login by themselves, so this is where they stop being a surprise. Each can be switched off for
  // Hub's threads only; a terminal keeps everything, and threads already running keep what they had.
  import { api } from "../api";
  import { ago } from "../format";
  import { openUrl } from "../native";
  import { hub } from "../state.svelte";
  import { engineProvider } from "../providers.svelte";
  import type { Connections } from "../bindings/Connections";
  import type { ConnectionPrefs } from "../bindings/ConnectionPrefs";
  import type { DoctorReport } from "../bindings/DoctorReport";
  import Switch from "./Switch.svelte";
  import { SOURCE, STATE } from "../connections";
  import ProviderMark from "./ProviderMark.svelte";

  let list = $state<Connections | null>(null);
  let busy = $state(false);
  let open = $state<string | null>(null);

  const load = async (refresh = false) => {
    busy = true;
    try {
      list = await api.connections(refresh);
    } catch (e) {
      hub.fail(e);
    } finally {
      busy = false;
    }
  };
  $effect(() => {
    load();
  });

  let prefs = $state<ConnectionPrefs>({ off: {}, account_off: [], plugins_off: {} });
  $effect(() => {
    quiet(api.connectionPrefs().then((p) => (prefs = p)), "connection prefs");
  });
  const toggled = (list: string[] | undefined, item: string, off: boolean) =>
    off ? [...(list ?? []), item] : (list ?? []).filter((x) => x !== item);
  async function save(next: ConnectionPrefs) {
    try {
      prefs = await api.setConnectionPrefs(next);
      for (const e of list?.engines ?? []) {
        e.account_off = prefs.account_off.includes(e.engine);
        for (const c of e.connections)
          c.off = c.source !== "hub" && ((prefs.off[e.engine] ?? []).includes(c.name) || (c.source === "claude_ai" && e.account_off));
        for (const p of e.plugins) p.off = (prefs.plugins_off[e.engine] ?? []).includes(p.id);
      }
    } catch (e) {
      hub.fail(e);
    }
  }
  const setConnection = (engine: string, name: string, on: boolean) =>
    save({ ...prefs, off: { ...prefs.off, [engine]: toggled(prefs.off[engine], name, !on) } });
  const setAccount = (engine: string, on: boolean) => save({ ...prefs, account_off: toggled(prefs.account_off, engine, !on) });
  const setPlugin = (engine: string, id: string, on: boolean) =>
    save({ ...prefs, plugins_off: { ...prefs.plugins_off, [engine]: toggled(prefs.plugins_off[engine], id, !on) } });

  let doctor = $state<Record<string, DoctorReport | "running">>({});
  async function runDoctor(engine: string) {
    doctor[engine] = "running";
    try {
      doctor[engine] = await api.engineDoctor(engine);
    } catch (e) {
      delete doctor[engine];
      hub.fail(e);
    }
  }
</script>

{#if list}
  {#each list.engines as e (e.engine)}
    {@const p = engineProvider(e.engine)}
    <section>
      <div class="head">
        <ProviderMark id={p.id} size={14} />
        <span class="name">{p.name}</span><span class="mono dim">{e.engine}</span>
        <span class="dim right">{e.connections.length} {e.connections.length === 1 ? "connection" : "connections"}</span>
      </div>
      {#if e.error}<div class="warn">Could not ask it ({e.error}), so this comes from its files and the threads.</div>{/if}
      {#if e.account}
        <div class="line">
          <div class="what">
            <div class="label">{e.account.startsWith("claude") ? e.account : e.account[0].toUpperCase() + e.account.slice(1)} in Hub's threads</div>
            <div class="dim">The ones on your {e.account.startsWith("claude") ? "claude.ai" : "ChatGPT"} account reach every thread by themselves. Off here, Hub's threads go without them; a terminal still has them.</div>
          </div>
          <Switch label="{e.account} in Hub's threads" on={!e.account_off} onchange={(on) => setAccount(e.engine, on)} />
        </div>
      {/if}
      {#each e.connections as c (c.name)}
        {@const key = `${e.engine}/${c.name}`}
        {@const s = STATE[c.state]}
        {@const byAccount = c.source === "claude_ai" && e.account_off}
        <div class="row" class:open={open === key} class:isoff={c.off}>
          <button class="expand" onclick={() => (open = open === key ? null : key)}>
            <span class="dot {s.tone}" data-tip={s.label}></span>
            <span class="label">{c.label}</span>
            <span class="pill">{SOURCE[c.source]}</span>
            {#if c.off}<span class="pill">Off in Hub</span>{/if}
            <span class="used dim">
              {#if c.uses}{c.uses} {c.uses === 1 ? "call" : "calls"}{#if c.last_used}, {ago(c.last_used)}{/if}{:else}Not used yet{/if}
            </span>
          </button>
          {#if c.source !== "hub"}
            <Switch label="{c.label} in Hub's threads" on={!c.off} disabled={byAccount}
              onchange={(on) => setConnection(e.engine, c.name, on)} />
          {:else}
            <span class="switch-gap"></span>
          {/if}
        </div>
        {#if open === key}
          <div class="more">
            <div class="kv"><span>Status</span><span class={s.tone}>{s.label}{#if c.detail}<span class="dim"> · {c.detail}</span>{/if}</span></div>
            {#if c.target}<div class="kv"><span>Where</span><span class="mono">{c.target}</span>{#if c.transport}<span class="dim"> {c.transport}</span>{/if}</div>{/if}
            <div class="kv"><span>Named</span><span class="mono">{c.name}</span></div>
            {#if c.tools.length}
              <div class="kv top">
                <span>Tools used</span>
                <span class="tools">
                  {#each c.tools as t (t.name)}
                    <span class="tool" data-tip="{t.count} {t.count === 1 ? 'call' : 'calls'}, last {ago(t.last_used)}">{t.name}{#if t.count > 1}<span class="dim"> ×{t.count}</span>{/if}</span>
                  {/each}
                </span>
              </div>
              <div class="kv"><span></span><span class="dim">In {c.threads} {c.threads === 1 ? "thread" : "threads"}. It can have more tools than these; Hub learns them as threads call them.</span></div>
            {:else}
              <div class="kv"><span>Tools used</span><span class="dim">None yet. Hub learns its tools as threads call them.</span></div>
            {/if}
            {#if c.manage_url}
              <div class="kv"><span></span><button class="link" onclick={() => openUrl(c.manage_url!)}>Manage on claude.ai</button></div>
            {/if}
          </div>
        {/if}
      {:else}
        <Quiet text="No connections" />
      {/each}
      {#if e.plugins.length}
        <div class="sub">Plugins</div>
        {#if e.plugins.some((p) => p.skills.length)}
          <p class="dim note">The engine lists every plugin's skills to the model in a small budget (2% of its context for Codex); too many and it drops descriptions and leaves skills out. Switch off what Hub's threads don't need.</p>
        {/if}
        {#each e.plugins as p (p.id)}
          <div class="row" class:isoff={p.off || !p.enabled}>
            <span class="expand static">
              <span class="label mono">{p.id}</span>
              {#if p.version}<span class="dim">{p.version}</span>{/if}
              {#if p.scope}<span class="pill">{p.scope}</span>{/if}
              {#if p.skills.length}<span class="dim" data-tip={p.skills.join(", ")}>{p.skills.length} {p.skills.length === 1 ? "skill" : "skills"}</span>{/if}
              {#if !p.enabled}<span class="pill">Off in {p.scope === "synced" ? "your account" : "its settings"}</span>{:else if p.off}<span class="pill">Off in Hub</span>{/if}
            </span>
            <Switch label="{p.id} in Hub's threads" on={!p.off} disabled={!p.enabled} onchange={(on) => setPlugin(e.engine, p.id, on)} />
          </div>
        {/each}
      {/if}
      {#if e.doctor}
        {@const d = doctor[e.engine]}
        <div class="doctor">
          <button class="link" disabled={d === "running"} onclick={() => runDoctor(e.engine)}>{d === "running" ? "Checking…" : `Run ${e.doctor}`}</button>
          {#if d && d !== "running"}
            <pre class:bad={!d.ok}>{d.output || "No output."}</pre>
          {/if}
        </div>
      {/if}
    </section>
  {/each}
  <p class="dim note">Switches apply to threads as their engine starts; a thread that is already running keeps what it had until it restarts.</p>
  <div class="foot">
    Asked with <span class="mono">claude mcp list</span> and <span class="mono">codex mcp list</span> from your home folder; a project's own servers show in its threads.
    Checked {ago(list.engines[0]?.checked_at ?? Date.now())}.
    <button class="link" disabled={busy} onclick={() => load(true)}>{busy ? "Checking…" : "Check again"}</button>
  </div>
{:else}
  <section><Quiet kind="loading" /></section>
{/if}

<style>
  section { padding: 12px 0; }
  section + section { border-top: 1px solid var(--line); }
  .head { display: flex; align-items: center; gap: 8px; margin-bottom: 6px; }
  .name { color: var(--text); font-size: 14px; }
  .dim { color: var(--muted); font-size: 12px; }
  .right { margin-left: auto; }
  .warn { color: var(--amber); font-size: 13px; margin: 2px 0 6px; }
  .row { display: flex; align-items: center; gap: 12px; width: calc(100% + 12px); margin: 0 -6px; padding: 2px 6px; border-radius: var(--r-sm); font-size: 13px; }
  .row:hover, .row.open { background: var(--surface); }
  .expand { display: flex; align-items: center; gap: 10px; flex: 1; min-width: 0; padding: 4px 0; text-align: left; font-size: 13px; }
  .expand.static { cursor: default; }
  .row.isoff .label { color: var(--muted); text-decoration: line-through; text-decoration-color: var(--line-strong); }
  .switch-gap { width: 32px; flex: none; }
  .line { display: flex; align-items: center; gap: 16px; padding: 4px 0 10px; }
  .what { flex: 1; min-width: 0; }
  .what .label { color: var(--text-2); font-size: 13px; }
  .what .dim { margin-top: 2px; }
  .sub { color: var(--muted); font-size: 12px; margin: 12px 0 4px; }
  .doctor { margin-top: 8px; font-size: 13px; }
  .doctor pre { margin: 6px 0 0; padding: 8px 10px; background: var(--surface); border-radius: var(--r-sm); font-family: var(--mono); font-size: 12px; color: var(--text-3); white-space: pre-wrap; max-height: 260px; overflow: auto; }
  .doctor pre.bad { color: var(--red); }
  .note { margin: 4px 0 0; }
  .dot { width: 7px; height: 7px; border-radius: 50%; flex: none; background: var(--muted); }
  .dot.ok { background: var(--green); }
  .dot.warn { background: var(--amber); }
  .dot.bad { background: var(--red); }
  .label { color: var(--text-2); min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .used { margin-left: auto; white-space: nowrap; }
  .pill { font-size: 11px; color: var(--text-3); background: var(--surface-hover); border-radius: var(--r-sm); padding: 1px 8px; white-space: nowrap; }
  .more { padding: 4px 0 10px 23px; font-size: 13px; }
  .kv { display: grid; grid-template-columns: 84px 1fr; gap: 10px; padding: 2px 0; color: var(--text-2); align-items: baseline; }
  .kv > span:first-child { color: var(--muted); }
  .kv .mono { overflow-wrap: anywhere; }
  .ok { color: var(--green); }
  span.warn { color: var(--amber); }
  .bad { color: var(--red); }
  .tools { display: flex; flex-wrap: wrap; gap: 4px; }
  .tool { font-family: var(--mono); font-size: 12px; background: var(--surface-hover); border-radius: var(--r-sm); padding: 1px 6px; color: var(--text-2); }
  .empty { color: var(--muted); padding: 4px 0; font-size: 13px; }
  .foot { color: var(--muted); font-size: 13px; line-height: 1.7; padding: 8px 0 4px; }
  .link { color: var(--accent); font-size: inherit; padding: 0 2px; }
  .kv .link { justify-self: start; padding: 0; }
  .link:hover { text-decoration: underline; }
</style>
