<!-- Settings page: about. Used by Settings.svelte. -->
<script lang="ts">
  import { home, useHubAt, useOwnHub } from "../../home";
  import { sc, restart, open, openFolder } from "./core.svelte";
  import { BASE } from "../../api";
  import { hub } from "../../state.svelte";
  import Icon from "../Icon.svelte";
  import Switch from "../Switch.svelte";

  const about = $derived(sc.about);
  const core = $derived(sc.core);
  const restarting = $derived(sc.restarting);

  // Which Hub this window uses: this computer's own, or the one on another machine.
  let hubAddress = $state("");

  let hubCode = $state("");

  let joining = $state(false);

  async function joinHub() {
    joining = true;
    try {
      await useHubAt(hubAddress, hubCode);
      location.reload();
    } catch (e) {
      hub.fail(e);
      joining = false;
    }
  }

  async function ownHub() {
    await useOwnHub();
    location.reload();
  }

  const shortcuts: [string, string][] = [
    ["Ctrl N", "New thread"],
    ["Ctrl K", "Search"],
    ["Ctrl J", "Next thread waiting on you"],
    ["Ctrl B", "Toggle the sidebar"],
    ["Ctrl E", "Switch between Inbox (threads) and System (projects and machines)"],
    ["Alt 0 / 1 / 2", "Home, Inbox, System, as on the rail"],
    ["Ctrl Z", "Undo the last settle, outside a text field"],
    ["Ctrl .", "Toggle the right panel"],
    ["Ctrl ,", "Settings"],
    ["Ctrl U", "Add files to a message"],
    ["Ctrl D", "Dictate into the message box"],
    ["Ctrl 1–9", "Open the threads in the Inbox list, top first"],
    ["Ctrl F", "Find in the open thread; Enter and Shift Enter move between matches"],
    ["F2", "Rename the open thread"],
    ["Ctrl Shift T", "Bring back what you last closed in the right panel"],
    ["Ctrl Enter", "Send as a new thread in the same folder, started in the background; you stay where you are"],
    ["Ctrl S", "Put the message away for later; in an empty box, take it back"],
    ["Arrow Up", "In an empty message box, what you sent before"],
    ["Ctrl Shift V", "Paste a big text as text, not as an attached file"],
    ["Ctrl + / −", "Zoom, also Ctrl and the mouse wheel"],
    ["Ctrl 0", "Reset zoom"],
  ];
</script>

<h1>About and help</h1>
<section>
  <div class="facts">
    <span class="k">Version</span><span class="v mono">{about?.version ?? "…"}</span>
    <span class="k">System</span><span class="v mono">{about?.os ?? ""}{about?.user ? ` · ${about.user}` : ""}</span>
    <span class="k">Data</span><button class="v mono link" onclick={() => about && openFolder(about.data_dir)}>{about?.data_dir ?? ""}</button>
    <span class="k">Config</span><span class="v mono">{about?.config_path ?? ""}{about && !about.config_exists ? " (not created, defaults apply)" : ""}</span>
  </div>
  {#if about?.config_problem}
    <p class="hint problem">Hub runs on default settings: {about.config_problem}</p>
  {/if}
  {#if about?.repo_url}
    <div class="actions">
      <button class="btn" onclick={() => open(`${about!.repo_url}/blob/main/docs/ARCHITECTURE.md`)}>How Hub works <Icon name="external" size={13} /></button>
      <button class="btn" onclick={() => open(`${about!.repo_url}/issues`)}>Issues <Icon name="external" size={13} /></button>
      <button class="btn" onclick={() => open(about!.repo_url!)}>Repository <Icon name="external" size={13} /></button>
    </div>
  {/if}
</section>
<section>
  <div class="line">
    <div class="what"><div class="name">Core</div><div class="hint">Runs threads. Keeps going when the window closes.</div></div>
    <div class="control">
      <span class="dot" style:background={hub.connected ? "var(--green)" : "var(--red)"}></span>
      <span class="value mono">{BASE.replace(/^https?:\/\//, "")}</span>
      <button class="btn" disabled={!hub.connected || restarting} data-tip={core?.busy ? "Working threads carry on after it" : "Start the core again"} onclick={restart}>Restart</button>
    </div>
  </div>
  {#if home}
    <div class="line">
      <div class="what">
        <div class="name">Uses the Hub on {home.name}</div>
        <div class="hint">Shows the threads, projects and settings of {home.name}.</div>
      </div>
      <div class="control"><button class="btn" onclick={ownHub}>Use this computer's own Hub</button></div>
    </div>
  {:else}
    <div class="line">
      <div class="what">
        <div class="name">Use the Hub on another machine</div>
        <div class="hint">The same threads, projects and settings as there. Make a code there in Settings > Machines.</div>
      </div>
      <div class="control">
        <input class="path short mono" bind:value={hubAddress} placeholder="pc.tail1234.ts.net" spellcheck="false" />
        <input class="path short mono" bind:value={hubCode} placeholder="Code" spellcheck="false" autocomplete="off" />
        <button class="btn" disabled={joining || !hubAddress.trim() || !hubCode.trim()} onclick={joinHub}>{joining ? "Connecting…" : "Connect"}</button>
      </div>
    </div>
  {/if}
</section>
<h2>Keyboard shortcuts</h2>
<section class="list">
  {#each shortcuts as [keys, what] (keys)}
    <div class="change"><span class="subject">{what}</span><kbd>{keys}</kbd></div>
  {/each}
</section>

<style>
  /* @kit settings */
</style>
