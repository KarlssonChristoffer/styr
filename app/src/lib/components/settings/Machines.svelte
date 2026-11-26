<!-- Settings page: machines. Used by Settings.svelte. -->
<script lang="ts">
  import { onMount } from "svelte";
  import { openFolder } from "./core.svelte";
  import Quiet from "../Quiet.svelte";
  import { api } from "../../api";
  import type { SshInfo } from "../../bindings/SshInfo";
  import type { LanMachine } from "../../bindings/LanMachine";
  import { ago, shortVersion } from "../../format";
  import { openUrl } from "../../native";
  import { hub } from "../../state.svelte";
  import HostChip from "../HostChip.svelte";
  import Icon from "../Icon.svelte";
  import { menu } from "../../menu.svelte";
  import { machineKindMenu } from "../../menus.svelte";

  let ssh = $state<SshInfo | null>(null);
  function loadSsh() {
    api.ssh().then((s) => (ssh = s)).catch((e) => hub.fail(e));
  }
  onMount(() => {
    loadSsh();
    hub.refreshTailnet();
  });
  // The core may restart (an update, or new settings); read again once it is back.
  let wasConnected = hub.connected;
  $effect(() => {
    const now = hub.connected;
    if (now && !wasConnected) loadSsh();
    wasConnected = now;
  });
  // Once a machine is signed in, it is on the tailnet list.
  $effect(() => {
    if (hub.hosts.some((h) => h.join && !h.join.failed)) return;
    hub.refreshTailnet();
  });

  // Machines: hosts from the SSH config can be added with one click; anything else by URL, with
  // a pairing code from that machine.
  let newName = $state("");

  let newUrl = $state("");

  let newCode = $state("");

  let adding = $state<string | null>(null);

  // Where an SSH alias really goes: its HostName, for every name on its Host line. Two machines
  // that go to the same place are the same machine.
  const target = (alias: string) => {
    const h = (ssh?.hosts ?? []).find((h) => h.alias === alias || h.aliases.includes(alias));
    return (h?.hostname ?? alias).toLowerCase();
  };

  const sshOf = (m: { address: string }) => m.address.split(" · ").find((a) => a.startsWith("ssh "))?.slice(4) ?? null;

  const sameAs = (m: (typeof hub.hosts)[number]) => {
    const alias = sshOf(m);
    if (!alias) return null;
    return hub.hosts.find((o) => o !== m && sshOf(o) && target(sshOf(o)!) === target(alias) && hub.hosts.indexOf(o) < hub.hosts.indexOf(m))?.name ?? null;
  };

  const unadded = $derived(
    (ssh?.hosts ?? []).filter(
      (h) => !h.alias.includes("*") && !hub.hosts.some((m) => m.name === h.alias || (sshOf(m) !== null && target(sshOf(m)!) === target(h.alias))),
    ),
  );

  let installing = $state<string | null>(null);

  // Before an install starts, a machine may ask two things, each once: whether this PC builds
  // hubd for it, and its sudo password for packages. The password goes to sudo there and is kept
  // nowhere.
  let ask = $state<{ name: string; sudo: string | null; build: string | null; join?: boolean } | null>(null);

  let sudoPassword = $state("");

  async function installOn(name: string, answer: { password?: string; build_here?: boolean } = {}) {
    installing = name;
    try {
      const reply = await api.installHost(name, answer);
      ask = reply.sudo || reply.build ? { name, sudo: reply.sudo, build: reply.build } : null;
    } catch (e) {
      hub.fail(e);
    } finally {
      installing = null;
      sudoPassword = "";
    }
  }

  // Machines on the tailnet set up with one click: added over SSH, Hub installed when missing.
  let sshUser = $state<Record<string, string>>({});

  let settingUp = $state<string | null>(null);

  async function setUp(key: string, name: string, ssh: string, password?: string) {
    settingUp = key;
    const user = (sshUser[key] ?? "").trim();
    try {
      await hub.setUpMachine({ name, ssh: user && !ssh.includes("@") ? `${user}@${ssh}` : ssh }, password);
    } catch (e) {
      hub.fail(e);
    } finally {
      settingUp = null;
      loginPassword = "";
    }
  }

  let loginPassword = $state("");

  // A machine whose SSH refuses this PC's key: its login password once puts the key there.
  let keyPassword = $state<Record<string, string>>({});

  async function letIn(name: string) {
    installing = name;
    try {
      await api.letIn(name, keyPassword[name] ?? "");
    } catch (e) {
      hub.fail(e);
    } finally {
      installing = null;
      keyPassword = { ...keyPassword, [name]: "" };
    }
  }

  // The home network is looked at only when asked: it tries every address there.
  let lan = $state<LanMachine[] | null>(null);

  let looking = $state(false);

  async function lookAround() {
    looking = true;
    try {
      lan = await api.lan();
    } catch (e) {
      hub.fail(e);
    } finally {
      looking = false;
    }
  }

  // A machine SSH reaches but that is not on the tailnet: Hub installs Tailscale there, and the
  // row shows the link to sign it in.
  const offTailnet = (h: (typeof hub.hosts)[number]) =>
    !h.local && !!sshOf(h) && !!hub.tailnet?.running && !hub.tailnet.peers.some((p) => p.machine === h.name);

  async function joinTailnet(name: string, password?: string) {
    installing = name;
    try {
      const reply = await api.joinTailnet(name, password ? { password } : {});
      ask = reply.sudo ? { name, sudo: reply.sudo, build: null, join: true } : null;
    } catch (e) {
      hub.fail(e);
    } finally {
      installing = null;
      sudoPassword = "";
    }
  }

  async function addMachine(name: string, how: { ssh?: string; url?: string; code?: string }) {
    adding = name;
    try {
      const info = await api.addHost({ name, ...how });
      if (!hub.hosts.some((h) => h.name === info.name)) hub.hosts.push(info);
      newName = newUrl = newCode = "";
    } catch (e) {
      hub.fail(e);
    } finally {
      adding = null;
    }
  }

  async function removeMachine(name: string) {
    try {
      await api.removeHost(name);
      hub.hosts = hub.hosts.filter((h) => h.name !== name);
    } catch (e) {
      hub.fail(e);
    }
  }

  const stateLabel = { online: "Online", connecting: "Connecting…", offline: "Offline" } as const;
</script>

<h1>Machines and SSH</h1>
<p class="lead">
  Where threads can run. Each machine runs its own hubd, so its threads keep going when this one sleeps.
</p>
<section class="list">
  {#each hub.hosts as h (h.name)}
    <div class="change machine" use:menu={() => machineKindMenu(h.name, h.local)}>
      <span class="subject">
        <button class="open-machine" title="Open {h.name}'s page" onclick={() => hub.openMachine(h.name)}>
          <HostChip name={h.name} />
          <span class="dim mono">{h.local ? "this machine" : h.address}</span>
          <Icon name="chevron" size={12} />
        </button>
        {#if h.error}<div class="why">{h.error}</div>{/if}
        {#if sameAs(h)}<div class="why">Goes to the same machine as {sameAs(h)}; one of them can go.</div>{/if}
        {#if h.join && !(ask?.name === h.name && ask.join)}
          <div class="why">
            Tailscale: {h.join.step}{h.join.failed ? "" : "…"}
            {#if h.join.login_url}<button class="link" onclick={() => openUrl(h.join!.login_url!)}>Sign {h.name} in to your tailnet</button>{/if}
          </div>
        {/if}
        {#if ask?.name === h.name && ask.build}
          <div class="why">
            {ask.build} This PC can build hubd for it in a few minutes and send it, and its updates too, so it needs
            neither Rust nor a GitHub login. That installs zig and cargo-zigbuild here once. Or {h.name} builds it
            itself: up to an hour on a Raspberry Pi, and updates need a GitHub login there.
          </div>
          <div class="sudo">
            <button class="btn small" disabled={installing === h.name} onclick={() => installOn(h.name, { build_here: true })}>Build on this PC</button>
            <button class="btn small" disabled={installing === h.name} onclick={() => installOn(h.name, { build_here: false })}>{h.name} builds it</button>
            <button class="btn small" onclick={() => (ask = null)}>Cancel</button>
          </div>
        {:else if h.setup === "no_key"}
          <div class="why">Its login password, once, puts this PC's key there. Never kept. Turn on Remote Login first on a Mac (System Settings &gt; General &gt; Sharing).</div>
          <form class="sudo" onsubmit={(e) => { e.preventDefault(); letIn(h.name); }}>
            <input class="path short" type="password" bind:value={keyPassword[h.name]} placeholder="Its login password" autocomplete="off" />
            <button class="btn small" disabled={!keyPassword[h.name] || installing === h.name}>{installing === h.name ? "Logging in…" : "Let this PC in"}</button>
          </form>
        {:else if ask?.name === h.name && ask.sudo}
          <div class="why">{ask.sudo} Never kept.</div>
          <form class="sudo" onsubmit={(e) => { e.preventDefault(); if (ask?.join) joinTailnet(h.name, sudoPassword); else installOn(h.name, { password: sudoPassword }); }}>
            <input class="path short" type="password" bind:value={sudoPassword} placeholder="sudo password" autocomplete="off" />
            <button class="btn small" disabled={!sudoPassword || installing === h.name}>{installing === h.name ? "Starting…" : ask.join ? "Add to tailnet" : "Install"}</button>
            <button class="btn small" type="button" onclick={() => { ask = null; sudoPassword = ""; }}>Cancel</button>
          </form>
        {/if}
      </span>
      {#if h.version}<span class="when mono" data-tip="hubd version">{shortVersion(h.version)}</span>{/if}
      {#if h.setup === "installing"}
        <span class="pill">Installing…</span>
      {:else}
        <span class="pill" class:ok={h.state === "online"} class:warn={h.state === "offline"}>{stateLabel[h.state]}</span>
      {/if}
      {#if h.setup === "installing" && ask?.name !== h.name}
        <button class="btn small" disabled={installing === h.name} data-tip="Stops the install that runs there and starts it again with this PC's version"
          onclick={() => installOn(h.name)}>{installing === h.name ? "Starting…" : "Start over"}</button>
      {/if}
      {#if (h.setup === "missing" || h.setup === "failed") && ask?.name !== h.name}
        <button class="btn small" disabled={installing === h.name} data-tip="Sends the installer over SSH and runs it there; it keeps itself up to date after that"
          onclick={() => installOn(h.name)}>{installing === h.name ? "Starting…" : h.setup === "failed" ? "Continue install" : "Install hubd"}</button>
      {/if}
      {#if offTailnet(h) && (!h.join || h.join.failed) && ask?.name !== h.name}
        <button class="btn small" disabled={installing === h.name} data-tip="Installs Tailscale there over SSH and gives you the link to sign it in, so it is reachable from everywhere"
          onclick={() => joinTailnet(h.name)}>{installing === h.name ? "Starting…" : "Add to tailnet"}</button>
      {/if}
      {#if h.state === "offline" && (!h.setup || h.setup === "no_key")}
        <button class="btn small" onclick={() => api.reconnectHost(h.name).catch((e) => hub.fail(e))}>Retry</button>
      {/if}
      {#if h.removable}
        <button class="btn small" data-tip="Its threads stay on it" onclick={() => removeMachine(h.name)}>Remove</button>
      {/if}
    </div>
  {/each}
</section>

{#if hub.machineAsk}
  <section class="list">
    <div class="change">
      <span class="subject">
        <span class="host">{hub.machineAsk.name || hub.machineAsk.ssh}</span>
        <div class="why">{hub.machineAsk.question} Never kept.</div>
        <form class="sudo" onsubmit={(e) => { e.preventDefault(); const a = hub.machineAsk!; setUp(a.ssh, a.name, a.ssh, loginPassword); }}>
          <input class="path short" type="password" bind:value={loginPassword} placeholder="Its password" autocomplete="off" />
          <button class="btn small" disabled={!loginPassword || settingUp !== null}>{settingUp ? "Logging in…" : "Set up"}</button>
          <button class="btn small" type="button" onclick={() => { hub.machineAsk = null; loginPassword = ""; }}>Cancel</button>
        </form>
      </span>
    </div>
  </section>
{/if}

<h2 class="with-action">On your tailnet <button class="link" onclick={() => hub.refreshTailnet()}>Look again</button></h2>
<section class="list">
  {#each hub.tailnet?.peers ?? [] as p (p.name)}
    <div class="change">
      <span class="subject">
        <span class="host">{p.name}</span> <span class="dim mono">{p.dns || p.ip}</span> <span class="dim">· {p.os}</span>
        {#if p.machine}<div class="why">In Hub{p.machine !== p.name ? ` as ${p.machine}` : ""}.</div>
        {:else if !p.supported}<div class="why">Hub runs threads on Linux machines and Macs.</div>{/if}
      </span>
      {#if !p.online}<span class="pill warn">Offline</span>{/if}
      {#if p.supported && !p.machine}
        <input class="path short" bind:value={sshUser[p.name]} placeholder="SSH user" spellcheck="false" data-tip="Who to log in as; empty uses your SSH config or this machine's user name" />
        <button class="btn small" disabled={settingUp !== null || !p.online}
          data-tip={p.os.toLowerCase() === "macos" ? `Over SSH to ${p.ssh}: adds it, and installs Hub there when it has none. Turn on Remote Login on the Mac first (System Settings > General > Sharing)` : `Over SSH to ${p.ssh}: adds it, and installs Hub there when it has none, built on this machine`}
          onclick={() => setUp(p.name, p.name.toLowerCase(), p.ssh)}>{settingUp === p.name ? "Starting…" : "Set up"}</button>
      {/if}
    </div>
  {:else}
    <div class="empty">{hub.tailnet?.error ?? (hub.tailnet ? "Nothing else on the tailnet." : "Loading…")}</div>
  {/each}
</section>

<h2 class="with-action">On your home network <button class="link" disabled={looking} onclick={lookAround}>{looking ? "Looking…" : lan ? "Look again" : "Look"}</button></h2>
<section class="list">
  {#each lan ?? [] as m (m.ip)}
    <div class="change">
      <span class="subject">
        <span class="host mono">{m.ip}</span> <span class="dim">{m.system || m.banner}</span>
        {#if m.machine}<div class="why">In Hub as {m.machine}.</div>{/if}
      </span>
      {#if !m.machine}
        <input class="path short" bind:value={sshUser[m.ip]} placeholder="SSH user" spellcheck="false" data-tip="Who to log in as, e.g. pi; empty uses your SSH config or this machine's user name" />
        <button class="btn small" disabled={settingUp !== null}
          data-tip="Logs in over SSH (its password once if it does not know this PC's key), names it after its host name, puts it on your tailnet and installs Hub there"
          onclick={() => setUp(m.ip, "", m.ip)}>{settingUp === m.ip ? "Starting…" : "Set up"}</button>
      {/if}
    </div>
  {:else}
    <div class="empty">None.</div>
  {/each}
</section>

<h2>Add from the SSH config</h2>
<section class="list">
  {#each unadded as h (h.alias)}
    <div class="change">
      <span class="subject"><span class="host">{h.alias}</span>{#if h.aliases.length}<span class="dim"> · {h.aliases.join(", ")}</span>{/if} <span class="dim mono">{h.user ? `${h.user}@` : ""}{h.hostname ?? h.alias}</span></span>
      <button class="btn small" disabled={adding === h.alias} onclick={() => addMachine(h.alias, { ssh: h.alias })}>
        {adding === h.alias ? "Adding…" : "Add"}
      </button>
    </div>
  {:else}
    {#if ssh}<Quiet text="None" />{:else}<Quiet kind="loading" />{/if}
  {/each}
</section>

<h2>Add by URL</h2>
<section>
  <div class="line">
    <div class="what">
      <div class="name">A machine where hubd answers directly</div>
      <div class="hint">
        Run <span class="mono">hubd pair --tailscale</span> there; it prints its address and a one-time code. A machine
        already added by SSH can be added again by URL, and keeps the tunnel as a fallback.
      </div>
    </div>
    <div class="control">
      <input class="path short" bind:value={newName} placeholder="Name" spellcheck="false" />
      <input class="path" bind:value={newUrl} placeholder="https://tincan.….ts.net" spellcheck="false" />
      <input class="path code mono" bind:value={newCode} placeholder="Code" spellcheck="false" />
      <button class="btn" disabled={!newName.trim() || !newUrl.trim() || adding !== null}
        onclick={() => addMachine(newName.trim(), { url: newUrl.trim(), code: newCode.trim() || undefined })}>Add</button>
    </div>
  </div>
</section>
<div class="foot">
  hubd on a Linux machine: <span class="mono">scripts/install-host.sh</span> from the repository, run there. It
  installs a user service that keeps itself up to date, as on this machine, and with Tailscale prints a code.
</div>

<h2 class="with-action">SSH keys <button class="link mono" onclick={() => ssh && openFolder(ssh.dir)}>{ssh?.dir ?? "~/.ssh"}</button></h2>
<section class="list">
  {#each ssh?.keys ?? [] as k (k.file)}
    <div class="change">
      <span class="hash mono">{k.kind.replace(/^ssh-/, "")}</span>
      <span class="subject">{k.file.replace(/\.pub$/, "")}{#if k.comment} <span class="dim">{k.comment}</span>{/if}</span>
      {#if !k.has_private}<span class="pill off" data-tip="Only the public half is here">Public only</span>{/if}
      <span class="when">{k.modified ? ago(k.modified) : ""}</span>
    </div>
  {:else}
    {#if ssh}<Quiet text="No keys" />{:else}<Quiet kind="loading" />{/if}
  {/each}
</section>
<h2>SSH hosts</h2>
<section class="list">
  {#each ssh?.hosts ?? [] as h (h.alias)}
    <div class="change">
      <span class="subject"><span class="host">{h.alias}</span> <span class="dim mono">{h.user ? `${h.user}@` : ""}{h.hostname ?? h.alias}{h.port ? `:${h.port}` : ""}</span></span>
      {#if h.proxy_jump}<span class="pill">via {h.proxy_jump}</span>{/if}
      <span class="when mono">{h.identity_file?.replace(/^.*[\\/]/, "") ?? ""}</span>
    </div>
  {:else}
    {#if ssh}<Quiet text="No hosts in the SSH config" />{:else}<Quiet kind="loading" />{/if}
  {/each}
</section>
{#if ssh}<div class="foot">{ssh.known_hosts} known hosts</div>{/if}

<style>
  /* @kit settings */
</style>
