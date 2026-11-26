<!-- The GitHub connection in Settings > Account, one line per machine: who is signed in, with what scopes, and Connect / Reconnect / Sign out. It is `gh`'s sign-in on that machine (`github_repos.rs`); Hub holds no token. A hubd from before feature `github.account` sends the login only (no `scopes`), and then there is no Sign out.
     Used by Settings.svelte. -->
<script lang="ts">
  import { onMount } from "svelte";
  import { poll } from "../poll";
  import { api } from "../api";
  import { hub } from "../state.svelte";
  import ProviderLogo from "./ProviderLogo.svelte";
  import { connect, gh, giveUp, look, signOut, type MachineState } from "../github.svelte";

  const hosts = $derived(hub.hosts.filter((h) => h.state === "online").map((h) => ({ name: h.name, host: h.local ? null : h.name })));
  const stateOf = (host: string | null): MachineState | undefined => gh.machines[host ?? ""];
  let busy = $state<string | null>(null);

  const key = (host: string | null) => host ?? "";
  const later = (s: MachineState) => s.state === "old";
  /** `repo`, `read:org`: what that sign-in may do, as gh reports it. */
  const scopes = (s: MachineState) => (s.scopes ?? []).join(", ");

  async function lookAll() {
    await Promise.all(hosts.map((h) => look(h.host)));
  }

  async function act(host: string | null, run: () => Promise<unknown>) {
    busy = key(host);
    try {
      await run();
    } finally {
      busy = null;
    }
  }

  onMount(() => {
    lookAll();
    // While a code waits to be typed on GitHub, look again until it is done.
    return poll(() => {
      for (const h of hosts) if (stateOf(h.host)?.state === "waiting") look(h.host);
    }, 2500);
  });
</script>

<section>
  {#each hosts as h (h.name)}
    {@const s = stateOf(h.host)}
    <div class="line">
      {#if s?.avatar}<img class="av" src={s.avatar} alt="" />{:else}<ProviderLogo id="github" size={20} />{/if}
      <div class="what">
        <div class="name">GitHub{hosts.length > 1 ? ` on ${h.name}` : ""}</div>
        {#if !s}
          <div class="hint">Looking…</div>
        {:else if s.state === "ready"}
          <div class="hint">
            {s.name ? `${s.name} · ` : ""}<span class="mono">{s.login ?? "signed in"}</span>
            {#if scopes(s)} · {scopes(s)}{/if}
          </div>
        {:else if s.state === "waiting"}
          <div class="hint">
            Type <span class="mono code">{s.code}</span> at <span class="mono">github.com/login/device</span>.
            <button class="link" onclick={() => api.openUrl(s.url ?? "https://github.com/login/device").catch((e) => hub.fail(e))}>Open</button>
          </div>
        {:else if s.state === "lost"}
          <div class="hint">{s.message ?? "GitHub no longer accepts the saved sign-in."}</div>
        {:else if s.state === "missing"}
          <div class="hint">{s.message ?? "Install the GitHub CLI (gh) here."}</div>
        {:else if later(s)}
          <div class="hint">Needs the next version of Hub on this machine.</div>
        {:else if s.state === "failed"}
          <div class="hint">{s.message ?? "Did not finish."}</div>
        {:else}
          <div class="hint">{s.message ?? "Not signed in."}</div>
        {/if}
      </div>
      {#if s}
        <div class="control">
          {#if s.state === "ready"}
            <span class="pill ok">Connected</span>
            <button class="btn" disabled={busy === key(h.host)} onclick={() => act(h.host, () => connect(h.host))}>Reconnect</button>
            {#if s.scopes}<button class="btn" disabled={busy === key(h.host)} onclick={() => act(h.host, () => signOut(h.host))}>Sign out</button>{/if}
          {:else if s.state === "waiting"}
            <span class="pill">Waiting</span>
            <button class="btn" onclick={() => giveUp(h.host)}>Cancel</button>
          {:else if s.state === "lost"}
            <span class="pill warn">Sign-in lost</span>
            <button class="btn primary" disabled={busy === key(h.host)} onclick={() => act(h.host, () => connect(h.host))}>Reconnect</button>
          {:else if s.state === "none" || s.state === "failed"}
            <span class="pill off">Not connected</span>
            <button class="btn primary" disabled={busy === key(h.host)} onclick={() => act(h.host, () => connect(h.host))}>Connect</button>
          {:else}
            <span class="pill off">{s.state === "missing" ? "No gh" : "Unavailable"}</span>
          {/if}
        </div>
      {/if}
    </div>
  {/each}
</section>

<style>
  .line { display: flex; flex-wrap: wrap; align-items: center; gap: 12px 16px; padding: 16px 0; }
  .line + .line { border-top: 1px solid var(--line); }
  .av { width: 32px; height: 32px; border-radius: 50%; flex: none; }
  .what { flex: 1; flex-basis: 260px; min-width: 0; }
  .name { color: var(--text); font-size: 14px; font-weight: 500; }
  .hint { color: var(--muted); font-size: 13px; margin-top: 3px; overflow-wrap: anywhere; }
  .control { display: flex; flex-wrap: wrap; align-items: center; gap: 6px; flex: none; max-width: 100%; }
  .code { color: var(--text); letter-spacing: 0.05em; }
  .link { background: none; border: 0; padding: 0; margin-left: 6px; color: var(--accent); cursor: pointer; font: inherit; }
  .pill { font-size: 12px; color: var(--text-3); background: var(--surface-hover); border-radius: var(--r-sm); padding: 1px 8px; white-space: nowrap; }
  .pill.ok { color: var(--green); background: var(--green-bg); }
  .pill.warn { color: var(--amber); background: var(--amber-bg); }
  .pill.off { color: var(--muted); }
</style>
