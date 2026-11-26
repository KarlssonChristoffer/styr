<!-- Settings: what you set up once rather than use every day. Each section reads where the thing already lives (the core's config, ~/.ssh, the engines' own logins); nothing secret is shown.
     Used by App.svelte. -->
<script lang="ts">
  import { quiet } from "../quiet";
  import Tabs from "./Tabs.svelte";
  import { sc, loadShared, edit, restart, open } from "./settings/core.svelte";
  import { modal } from "../pop";
  import Quiet from "./Quiet.svelte";
  // Settings: what you set up once rather than use every day. Each section reads where the thing
  // already lives (the core's config, ~/.ssh, the engines' own logins); nothing secret is shown.
  import { onMount, tick } from "svelte";
  import { api } from "../api";
  import type { About } from "../bindings/About";
  import { prefs } from "../prefs.svelte";
  import { hub, type SettingsSection } from "../state.svelte";
  import { useModalHistory } from "../modalHistory.svelte";
  import Icon from "./Icon.svelte";
  import { menu } from "../menu.svelte";
  import Switch from "./Switch.svelte";
  import NoticeSettings from "./NoticeSettings.svelte";
  import { load as ghLoad } from "../github.svelte";
  import WhereItGoes from "./WhereItGoes.svelte";
  import Customs from "./Customs.svelte";
  import Personality from "./Personality.svelte";
  import Handoffs from "./Handoffs.svelte";
  import { reloadEngines } from "../providers.svelte";
  import ConnectionList from "./ConnectionList.svelte";
  import ApiConnections from "./ApiConnections.svelte";
  import KeyVault from "./KeyVault.svelte";
  import AboutHub from "./settings/AboutHub.svelte";
  import Projects from "./settings/Projects.svelte";
  import Safety from "./settings/Safety.svelte";
  import Engines from "./settings/Engines.svelte";
  import Chat from "./settings/Chat.svelte";
  import Dictation from "./settings/Dictation.svelte";
  import Appearance from "./settings/Appearance.svelte";
  import Account from "./settings/Account.svelte";
  import Privacy from "./settings/Privacy.svelte";
  import Updates from "./settings/Updates.svelte";
  import WhatsNew from "./settings/WhatsNew.svelte";
  import Devices from "./settings/Devices.svelte";
  import Machines from "./settings/Machines.svelte";
  import { apis } from "../apis.svelte";

  // Ten rows, no group headings, so the list never scrolls. A row with several parts shows them
  // as tabs over the page; each tab is one of the old sections, so every link into Settings still lands.
  type Icon_ = "user" | "palette" | "bell" | "threads" | "mic" | "spark" | "shield" | "stamp" | "folder" | "brain" | "puzzle" | "link" | "server" | "keys" | "restart" | "log" | "lock" | "help" | "key" | "pen" | "chart";
  type Tab = { id: SettingsSection; label: string };
  type Row = { label: string; icon: Icon_; tabs: Tab[] };
  const rows: Row[] = [
    { label: "Account", icon: "user", tabs: [{ id: "account", label: "Account" }, { id: "spend", label: "Where it goes" }] },
    { label: "Appearance", icon: "palette", tabs: [{ id: "appearance", label: "Appearance" }] },
    { label: "Chat", icon: "threads", tabs: [{ id: "chat", label: "Chat" }, { id: "dictation", label: "Voice" }] },
    { label: "Notifications", icon: "bell", tabs: [{ id: "notifications", label: "Notifications" }] },
    { label: "Engines", icon: "spark", tabs: [{ id: "engines", label: "Engines" }, { id: "handoffs", label: "Handoffs" }, { id: "personality", label: "Personality" }] },
    { label: "Safety", icon: "shield", tabs: [{ id: "safety", label: "Safety" }, { id: "customs", label: "Customs" }] },
    { label: "Projects", icon: "folder", tabs: [{ id: "projects", label: "Projects and tools" }] },
    { label: "Connections", icon: "link", tabs: [{ id: "apis", label: "APIs" }, { id: "vault", label: "Keys" }, { id: "extensions", label: "MCP and plugins" }] },
    { label: "Machines", icon: "server", tabs: [{ id: "machines", label: "Machines" }, { id: "access", label: "Devices" }] },
    { label: "Hub", icon: "restart", tabs: [{ id: "updates", label: "Updates" }, { id: "whatsnew", label: "What's new" }, { id: "privacy", label: "Data" }, { id: "about", label: "About" }] },
  ];
  const rowOf = (id: SettingsSection) => rows.find((r) => r.tabs.some((t) => t.id === id)) ?? rows[0];
  const pageOf = (id: SettingsSection) => {
    const r = rowOf(id);
    return { label: r.tabs.length > 1 ? `${r.label} · ${r.tabs.find((t) => t.id === id)!.label}` : r.label };
  };
  const row = $derived(rowOf(hub.settingsSection));

  // Every setting by name, so Find a setting reaches it without knowing the page.
  const index: [SettingsSection, string, string?][] = [
    ["account", "Your name", "profile display"],
    ["account", "Claude", "subscription login plan anthropic api key usage limits"],
    ["account", "Codex", "openai subscription login api key"],
    ["account", "GitHub", "gh login repository"],
    ["account", "Warn at", "quota guard limit usage"],
    ["account", "Switch to a cheaper model", "quota guard limit sonnet haiku"],
    ["account", "Carry on after a limit", "resume quota reset"],
    ["account", "Other keys", "api keys environment elevenlabs gemini"],
    ["apis", "API connections", "api keys credential manager jev typesafe classifier vault"],
    ["apis", "Add a connection", "custom api key bearer"],
    ["apis", "Model providers", "deepseek openrouter kimi zai xai provider key hermes opencode"],
    ["apis", "Your connections", "jev typesafe working status checked"],
    ["vault", "Your keys", "key vault secrets credential manager api keys rotate replace remove delete used by"],
    ["vault", "Add a key", "new secret environment variable"],
    ["vault", "Outside the vault", "environment variables keys"],
    ["appearance", "Theme", "dark light cappuccino colour color"],
    ["appearance", "Zoom", "size scale bigger smaller"],
    ["appearance", "Text size", "font bigger larger read far away eyes"],
    ["appearance", "Language", "svenska swedish english"],
    ["appearance", "Reduce motion", "animation slide fade"],
    ["appearance", "Show activity", "sidebar inbox menu"],
    ["appearance", "Code font", "monospace jetbrains terminal size"],
    ["appearance", "Chat font", "serif replies agent response typeface size newsreader"],
    ["appearance", "Chat look", "preset claude code compact hub"],
    ["appearance", "Line spacing", "line height chat"],
    ["chat", "Chat detail", "notes steps tool calls verbose quiet minimal full fold word"],
    ["chat", "Messages while it works", "follow-up queue steer"],
    ["chat", "Files open", "file page card inline panel modal side"],
    ["chat", "Math", "latex katex"],
    ["chat", "Working section", "activity fold"],
    ["chat", "Group by project", "activity folders"],
    ["chat", "Settle idle threads", "auto settle inactive days"],
    ["chat", "Settle when the PR merges", "auto settle merged pull request"],
    ["appearance", "Interface font", "typeface installed own"],
    ["engines", "Routines", "scheduled threads cron schedule recurring routine tasks"],
    ["engines", "New threads start on", "default engine model effort opus sonnet haiku claude codex"],
    ["engines", "What the models can do", "capabilities context vision images pdf tools reasoning web search litellm"],
    ["engines", "Except in", "per project exception per-project engine project defaults folder"],
    ["engines", "Local models", "ollama lm studio offline"],
    ["engines", "Auto-compact", "context compact compaction"],
    ["handoffs", "Pictures", "handoff side job image who draws codex gpt"],
    ["handoffs", "Plugin vetting", "handoff customs review who checks plugins hermes deepseek cheap model"],
    ["handoffs", "Research", "handoff side job look things up cheap model"],
    ["handoffs", "Code review", "handoff side job review another model"],
    ["handoffs", "Quick questions", "handoff side job haiku fast cheap"],
    ["handoffs", "Titles and summaries", "handoff tidy engine title tldr commit text housekeeping model"],
    ["personality", "Writing style", "output style short explains teaches explanatory learning personality tone"],
    ["safety", "Always allow", "permission rules allow bash tools settings.json"],
    ["safety", "Always refuse", "permission rules deny block bash tools"],
    ["notifications", "Focus time", "time focus do not disturb"],
    ["notifications", "Test notice", "send test toast windows"],
    ["extensions", "Connections", "mcp servers connectors claude.ai elevenlabs integrations tools"],
    ["extensions", "Plugins", "marketplace"],
    ["privacy", "Where Hub keeps its data", "folder database sqlite"],

    ["appearance", "Fonts", "typeface code"],
    ["notifications", "When a thread needs you", "desktop permission"],
    ["notifications", "Flash the taskbar", "blink"],
    ["notifications", "When a thread is done", "finished failed"],
    ["notifications", "Sound", "audio chime bell"],
    ["notifications", "Quiet hours", "do not disturb night evening"],
    ["notifications", "While I'm at the PC", "quiet present here keyboard"],
    ["notifications", "Pause notices", "mute snooze hour"],
    ["notifications", "Your own lines", "tell me when sentence rule custom"],
    ["notifications", "This PC", "windows device card"],
    ["notifications", "iPhone", "phone push device card"],
    ["dictation", "Dictation language", "voice whisper speech"],
    ["dictation", "Tidy what you said", "polish clean up punctuation haiku"],
    ["dictation", "Mute voice chat while dictating", "discord push to mute f13"],
    ["engines", "Model for tidying", "haiku handoff"],
    ["engines", "Harnesses", "your engines add engine harness hermes opencode lamps machines"],
    ["engines", "Providers", "keys provider deepseek openrouter gemini kimi"],
    ["safety", "Undo snapshots", "checkpoint rollback"],
    ["safety", "Second opinions by", "review model"],
    ["safety", "Proof before done", "tests screenshot"],
    ["safety", "Leak guard", "secrets tokens keys"],
    ["safety", "Clash warnings", "conflict same file"],
    ["safety", "Stay on the stage", "focus off topic park stage FOCUS.md"],
    ["safety", "Protected files", "env release scripts paths never edit"],
    ["safety", "Without asking, threads may", "default permissions mode read edit everything ask"],
    ["projects", "Folder for new projects", "path dev root"],
    ["projects", "Editor", "vs code open"],
    ["projects", "Terminal", "shell open"],
    ["projects", "Programs", "tools node git installed"],
    ["machines", "Machines", "hosts hubd tailscale remote"],
    ["machines", "Add by URL", "pair code"],
    ["machines", "SSH keys", "ssh ed25519"],
    ["machines", "SSH hosts", "ssh config"],
    ["access", "Your phone", "phone mobile qr tailnet tailscale"],
    ["access", "Devices", "pair token revoke"],
    ["access", "Let another device in", "pair code phone"],
    ["access", "Agents outside Hub", "hub tools agent keys mcp"],
    ["updates", "Install everywhere", "update machines versions claude code codex hermes cli hubd what's new"],
    ["updates", "Keep every machine on the same versions", "follow machines cli"],
    ["updates", "Update automatically", "version auto"],
    ["updates", "Ask before installing", "install"],
    ["updates", "Branch", "main"],
    ["updates", "Build folder", "disk space"],
    ["updates", "Disk", "space size storage"],
    ["whatsnew", "What's new", "patch notes release"],
    ["whatsnew", "Changelog", "commits history"],
    ["about", "Core", "restart hubd background"],
    ["about", "Keyboard shortcuts", "keys hotkeys"],
  ];
  // Back and forward step through the pages you opened here, not the page behind Settings.
  useModalHistory(
    () => ({ section: hub.settingsSection, api: hub.apiOpen, key: hub.keyOpen }),
    (p) => {
      hub.settingsSection = p.section;
      hub.apiOpen = p.api;
      hub.keyOpen = p.key;
    },
  );
  let query = $state("");
  const found = $derived.by(() => {
    const words = query.toLowerCase().split(/\s+/).filter(Boolean);
    if (!words.length) return [];
    return index
      .filter(([page, name, more]) => {
        const text = `${name} ${more ?? ""} ${pageOf(page).label}`.toLowerCase();
        return words.every((w) => text.includes(w));
      })
      .slice(0, 12);
  });

  /** Opens the page a setting is on and points at it. */
  async function goTo(page: SettingsSection, name: string) {
    query = "";
    hub.settingsSection = page;
    await tick();
    const el = [...document.querySelectorAll<HTMLElement>(".settings .body .name, .settings .body h2, .settings .body .l, .settings .body .rack-name, .settings .body .fold .title, .settings .body .lead .link, .settings .body .except .l")].find((n) => n.textContent?.trim().startsWith(name));
    const row = el?.closest<HTMLElement>(".line, .change, h2, .knob, .rack-row, .tidy, .fold, .lead") ?? el;
    if (!row) return;
    row.scrollIntoView({ block: "center" });
    row.classList.add("found");
    setTimeout(() => row.classList.remove("found"), 1600);
  }

  let { onclose }: { onclose: () => void } = $props();

  const about = $derived(sc.about);
  const accounts = $derived(sc.accounts);
  /** A latitude or longitude typed as text: kept only when it is a number within range. */
  const core = $derived(sc.core);
  const restarting = $derived(sc.restarting);

  /** Saved to hub.json but not yet used by the running core. */
  const pending = $derived(core ? JSON.stringify(core.running) !== JSON.stringify(core.saved) : false);
  const saved = $derived(core?.saved);

  /** Whether this Hub can hold a Rust build back by count or time (`update.modes`). */
  /** The times are on this window's clock, so Settings sends its offset with them. */


  /** Presses the mute key once, three seconds from now, for Discord's keybind recorder. */










  /** "3 worktrees, 12 branches, 218 temporary files", from what one clean-up removed. */




  function load() {
    quiet(ghLoad([null]), "GitHub login");
    loadShared();
  }

  // The core may restart (an update, or new settings); read everything again once it is back.
  let wasConnected = hub.connected;
  $effect(() => {
    const now = hub.connected;
    if (now && !wasConnected) {
      sc.restarting = false;
      load();
      reloadEngines();
    }
    wasConnected = now;
  });

  // Opened at a setting, e.g. Codex from its limits in the name menu: point at it once it shows.
  $effect(() => {
    const spot = hub.settingsSpot;
    if (!spot || (hub.settingsSection === "account" && !accounts)) return;
    hub.settingsSpot = null;
    goTo(hub.settingsSection, spot);
  });

  onMount(() => {
    load();
    quiet(apis.load(), "load");
  });









</script>


<svelte:window onkeydown={(e) => e.key === "Escape" && !e.defaultPrevented && onclose()} />

<!-- A window over the page, as in Claude: Esc, the cross or a click outside closes it. -->
<div role="presentation" class="backdrop" in:modal|global out:modal|global={{ out: true }} onclick={(e) => e.target === e.currentTarget && onclose()}>
<div class="settings" role="dialog" aria-modal="true" aria-label="Settings">
  <header>
    <span class="title">Settings</span>
    <button class="close" data-tip="Close (Esc)" onclick={onclose}><Icon name="x" /></button>
  </header>
  <div class="panes">
  <nav>
    <label class="find">
      <Icon name="search" size={14} />
      <input bind:value={query} placeholder="Find a setting" spellcheck="false"
        onkeydown={(e) => { if (e.key === "Escape" && query) { query = ""; e.preventDefault(); } else if (e.key === "Enter" && found[0]) goTo(found[0][0], found[0][1]); }} />
    </label>
    {#if query.trim()}
      {#each found as [page, name] (page + name)}
        <button class="row hit" onclick={() => goTo(page, name)}>
          <span class="ellipsis">{name}</span><span class="where">{pageOf(page).label}</span>
        </button>
      {:else}
        <Quiet text="Nothing called that" />
      {/each}
    {:else}
      {#each rows as r (r.label)}
        <button class="row" class:active={row === r}
          onclick={() => { hub.settingsSection = r.tabs[0].id; hub.apiOpen = null; hub.keyOpen = null; }}>
          <Icon name={r.icon} /><span>{r.label}</span>
        </button>
      {/each}
    {/if}
  </nav>

  <div class="body">
    <div class="inner">
      {#if row.tabs.length > 1}
        <Tabs items={row.tabs} value={hub.settingsSection} label={row.label}
          onchange={(id) => { hub.settingsSection = id; hub.apiOpen = null; hub.keyOpen = null; }} />
      {/if}
      {#if hub.settingsSection === "spend"}
        <h1>Where it goes</h1>
        <p class="lead">All your AI use.</p>
        <WhereItGoes />
      {:else if hub.settingsSection === "account"}
        <Account />

      {:else if hub.settingsSection === "appearance"}
        <Appearance />

      {:else if hub.settingsSection === "notifications"}
        <NoticeSettings />


      {:else if hub.settingsSection === "dictation"}
        <Dictation />

      {:else if hub.settingsSection === "chat"}
        <Chat />

      {:else if hub.settingsSection === "handoffs"}
        <h1>Handoffs</h1>
        <p class="lead">Model per kind of task.</p>
        <Handoffs />

      {:else if hub.settingsSection === "personality"}
        <h1>Personality</h1>
        <Personality />

      {:else if hub.settingsSection === "engines"}
        <Engines />

      {:else if hub.settingsSection === "safety"}
        <Safety />

      {:else if hub.settingsSection === "customs"}
        <Customs />

      {:else if hub.settingsSection === "projects"}
        <Projects />

      {:else if hub.settingsSection === "apis"}
        <ApiConnections />
      {:else if hub.settingsSection === "vault"}
        <KeyVault />

      {:else if hub.settingsSection === "extensions"}
        <h1>Extensions</h1>
        <p class="lead">MCP servers, skills and plugins.</p>
        <h2>Connections</h2>
        <ConnectionList />


      {:else if hub.settingsSection === "machines"}
        <Machines />

      {:else if hub.settingsSection === "access"}
        <Devices />

      {:else if hub.settingsSection === "updates"}
        <Updates />

      {:else if hub.settingsSection === "whatsnew"}
        <WhatsNew />

      {:else if hub.settingsSection === "privacy"}
        <Privacy />

      {:else}
        <AboutHub />

      {/if}
      {#if pending || restarting}
        <div class="banner">
          <span class="grow">
            {#if restarting}Restarting the core…
            {:else if core?.busy}Saved. Applies when the core restarts; threads that are working carry on after it.
            {:else}Saved. Some changes apply when the core restarts.{/if}
          </span>
          {#if !restarting}<button class="btn primary" onclick={restart}>Restart core</button>{/if}
        </div>
      {/if}
    </div>
  </div>
  </div>
</div>
</div>

<style>
  .backdrop { position: fixed; inset: 0; z-index: 50; background: rgba(0, 0, 0, 0.45); display: grid; place-items: center; padding: 32px; padding-top: max(32px, calc(var(--controls-h) + 8px)); }
  .settings {
    width: min(1440px, 100%); height: min(980px, 100%); display: flex; flex-direction: column; min-height: 0;
    background: var(--bg); border: 1px solid var(--line-strong); border-radius: var(--r-lg); overflow: hidden;
    box-shadow: 0 24px 64px rgba(0, 0, 0, 0.5);
  }
  header { display: flex; align-items: center; justify-content: space-between; height: 52px; flex: none; padding: 0 12px 0 22px; border-bottom: 1px solid var(--line); }
  .close { display: grid; place-items: center; width: 30px; height: 30px; border-radius: var(--r-md); color: var(--muted); }
  .close:hover { background: var(--surface-hover); color: var(--text); }
  .panes { flex: 1; display: flex; min-height: 0; }
  nav { width: 230px; flex: none; padding: 14px 12px; background: var(--bg-sidebar); overflow-y: auto; display: flex; flex-direction: column; gap: 1px; border-right: 1px solid var(--line); }
  .title { color: var(--text); font-weight: 600; font-size: 15px; }
  .row { display: flex; align-items: center; gap: 10px; height: 32px; padding: 0 10px; border-radius: var(--r-md); color: var(--text-3); text-align: left; }
  .row:hover { background: var(--surface); color: var(--text-2); }
  .row.active { background: var(--surface-hover); color: var(--text); }
  .row :global(svg) { flex: none; }
  .find { display: flex; align-items: center; gap: 8px; height: 30px; margin: 0 2px 8px; padding: 0 8px; border: 1px solid var(--line-strong); border-radius: var(--r-md); color: var(--muted); }
  .find:focus-within { border-color: var(--accent); }
  .find input { flex: 1; min-width: 0; background: none; border: 0; outline: none; font-size: 13px; }
  .row.hit { flex-direction: column; align-items: flex-start; justify-content: center; gap: 0; height: auto; padding: 4px 10px; }
  .where { color: var(--muted); font-size: 12px; }
  .ellipsis { max-width: 100%; white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
  :global(.settings .found) { animation: found 1.6s ease-out; border-radius: var(--r-sm); }
  @keyframes found { from { background: var(--accent-bg); } to { background: transparent; } }
  .body { flex: 1; overflow-y: auto; min-width: 0; }
  /* Flat, as in Claude's settings: rows run the whole width with the control at the right edge,
     sections are a heading and hairlines rather than boxes. */
  .inner { padding: 32px 40px 72px; }
  @media (max-width: 760px) { .backdrop { padding: 12px; padding-top: max(12px, calc(var(--controls-h) + 8px)); } nav { width: 60px; } nav .row span, .find { display: none; } }
  /* @kit settings */
</style>
