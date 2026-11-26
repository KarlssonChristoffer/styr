<!-- För: agent. A machine's "Ready for work" group: engines signed in, git identity, room on the disk, and the keys tried for real (MachineView). -->
<script lang="ts">
  import { api } from "../api";
  import { hub } from "../state.svelte";
  import { copy } from "../menu.svelte";
  import { openUrl } from "../native";
  import Quiet from "./Quiet.svelte";
  import MachineGroup from "./MachineGroup.svelte";
  import type { CredentialCheck } from "../bindings/CredentialCheck";
  import type { CredentialState } from "../bindings/CredentialState";
  import type { GitIdentity } from "../bindings/GitIdentity";
  import type { LoginJob } from "../bindings/LoginJob";
  import type { MachineReady } from "../bindings/MachineReady";
  import type { OpenOnMachine } from "../bindings/OpenOnMachine";
  import type { ReadyFix } from "../bindings/ReadyFix";

  let {
    name,
    at,
    online,
    local,
    ssh,
    ready,
    readyMissing,
    checking,
    mine,
    onread,
    onopen,
  }: {
    name: string;
    /** The machine to ask, null for this one. */
    at: string | null;
    online: boolean;
    local: boolean;
    ssh: string | null;
    ready: MachineReady | null;
    /** The machine's hubd is too old to say. */
    readyMissing: boolean;
    checking: boolean;
    /** Who git commits as on this computer, offered to a machine that has no one. */
    mine: GitIdentity | null;
    onread: () => Promise<void>;
    onopen: (what: OpenOnMachine) => Promise<void>;
  } = $props();

  const unready = $derived(ready?.checks.filter((c) => c.state !== "ok") ?? []);
  let gitName = $state("");
  let gitEmail = $state("");
  let gitBusy = $state(false);
  let hint = $state<string | null>(null);
  // Keys and sign-ins tried for real there (the core's `machine.credentials`).
  let creds = $state<CredentialCheck | null>(null);
  let credsBusy = $state(false);
  let credsOld = $state(false);
  async function testCreds() {
    if (!online || credsBusy) return;
    credsBusy = true;
    try {
      creds = await api.checkCredentials(at);
      credsOld = false;
    } catch {
      creds = null;
      credsOld = true;
    } finally {
      credsBusy = false;
    }
    // A sign-in that was just lost shows in the list above too.
    if (creds) void onread();
  }
  const CRED_SAYS: Record<CredentialState, string> = { ok: "Works", invalid: "Refused", missing: "Missing", unknown: "No answer" };
  function untilText(ms: number) {
    return new Date(ms).toLocaleDateString(undefined, { day: "numeric", month: "short", year: "numeric" });
  }
  async function fix(c: { fix: ReadyFix | null }) {
    const f = c.fix;
    if (!f) return;
    if (f.kind === "sign_in") await onopen({ what: "engine", engine: f.engine });
    else if (f.kind === "keep_signed_in") await onopen({ what: "sign_in", engine: f.engine });
    else if (f.kind === "shell") {
      hint = f.hint;
      await onopen({ what: "shell", engine: null });
    }
  }
  // An engine signing in there with a link and a code, shown here until it is done.
  let login = $state<LoginJob | null>(null);
  let loginTimer: ReturnType<typeof setTimeout> | undefined;
  async function signInHere(engine: string) {
    try {
      login = await api.startLogin(engine, at);
      followLogin(engine);
    } catch (e) {
      hub.fail(e);
    }
  }
  function followLogin(engine: string) {
    clearTimeout(loginTimer);
    loginTimer = setTimeout(async () => {
      login = (await api.loginJob(engine, at).catch(() => login)) ?? login;
      if (login?.running) followLogin(engine);
      else if (login?.ok) onread();
    }, 2000);
  }
  $effect(() => () => clearTimeout(loginTimer));

  async function setGit(id: GitIdentity) {
    gitBusy = true;
    try {
      await api.setGitIdentity(id, at);
      await onread();
    } catch (e) {
      hub.fail(e);
    } finally {
      gitBusy = false;
    }
  }
</script>

<MachineGroup machine={name} id="ready" title={unready.length ? `Ready for work · ${unready.length} to fix` : "Ready for work"} count={ready?.checks.length ?? 0}>
  <div class="readybox">
    <button class="link" disabled={checking} onclick={onread}>{checking ? "Checking…" : "Check again"}</button>
    {#if ready}
      <div class="checks">
        {#each ready.checks as c (c.key)}
          <div class="check {c.state}">
            <span class="t"><i class="state {c.state}"></i>{c.label}</span>
            <span class="d">{c.detail}</span>
            {#if c.fix?.kind === "git_identity"}
              {#if mine}
                <button class="btn small" disabled={gitBusy} data-tip="{mine.name} <{mine.email}>" onclick={() => setGit(mine!)}>Use {mine.name}</button>
              {:else}
                <form class="git" onsubmit={(e) => { e.preventDefault(); setGit({ name: gitName, email: gitEmail }); }}>
                  <input bind:value={gitName} placeholder="Name" />
                  <input bind:value={gitEmail} placeholder="Email" type="email" />
                  <button class="btn small" disabled={!gitName.trim() || !gitEmail.includes("@") || gitBusy}>Set</button>
                </form>
              {/if}
            {:else}
              {@render fixOf(c)}
            {/if}
          </div>
        {/each}
      </div>
      {#if hint}<p class="note">{hint}</p>{/if}
      <div class="creds">
        <div class="credhead">
          <span class="lb">Keys and sign-ins</span>
          <button class="link small" disabled={credsBusy} data-tip="Tries each one for real: starts every agent there and asks each service. Shows no key." onclick={testCreds}>{credsBusy ? "Trying…" : creds ? "Test again" : "Test keys and sign-ins"}</button>
        </div>
        {#if credsOld}
          <Quiet kind="newer" />
        {:else if creds}
          <div class="checks">
            {#each creds.rows as r (r.key)}
              <div class="check cred {r.state}">
                <span class="t"><i class="state {r.state}"></i>{r.label}</span>
                <span class="d" data-tip={r.detail}>{CRED_SAYS[r.state]}{r.until ? ` · until ${untilText(r.until)}` : ""}{r.state !== "ok" ? ` · ${r.detail}` : ""}</span>
                {#if r.fix && r.state !== "ok"}{@render fixOf({ fix: r.fix, state: r.state })}{/if}
              </div>
            {/each}
          </div>
        {/if}
      </div>
    {:else if readyMissing}
      <p class="muted">The hubd there is older than this check. Update hubd to see whether it is ready.</p>
    {:else}
      <p class="muted">Checking…</p>
    {/if}
  </div>
</MachineGroup>

{#snippet fixOf(c: { fix: ReadyFix | null; state: string })}

  {#if c.fix?.kind === "sign_in_here"}
                {@const f = c.fix}
                {#if login?.engine === f.engine && (login.running || login.ok === false)}
                  <div class="login">
                    {#if login.url && login.code}
                      <button class="link small" onclick={() => openUrl(login!.url!)}>Open {new URL(login.url).host}</button>
                      <span class="code mono" role="button" tabindex="0" data-tip="Copy" onclick={() => copy(login!.code!)} onkeydown={(e) => e.key === "Enter" && copy(login!.code!)}>{login.code}</span>
                    {/if}
                    <span class="d">{login.message}</span>
                    {#if !login.running}<button class="btn small" onclick={() => signInHere(f.engine)}>Try again</button>{/if}
                  </div>
                {:else}
                  <button class="btn small" onclick={() => signInHere(f.engine)}>Sign in</button>
                {/if}
              {:else if c.fix && (local || ssh)}
                {#if c.fix.kind === "keep_signed_in" && c.state === "ok"}
                  <button class="link small" data-tip="One sign-in, then every machine gets it" onclick={() => fix(c)}>Keep signed in for a year</button>
                {:else}
                  <button class="btn small" onclick={() => fix(c)}>{c.fix.kind === "sign_in" || c.fix.kind === "keep_signed_in" ? "Sign in there" : "Open terminal"}</button>
                {/if}
              {/if}
{/snippet}

<style>
  .readybox { padding: 0 8px; }
  .readybox > .link { margin: 0 0 4px; }
  .checks { display: flex; flex-direction: column; }
  .check { display: grid; grid-template-columns: auto minmax(0, 1fr); gap: 2px 10px; align-items: baseline; padding: 6px 0; border-bottom: 1px solid var(--line); min-width: 0; }
  .check .t { display: flex; gap: 8px; align-items: center; color: var(--text-2); font-size: 13px; white-space: nowrap; }
  .check .d { color: var(--muted); font-size: 12px; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; text-align: right; }
  .check.warn .d { color: var(--amber); }
  .check.missing .d { color: var(--red); }
  .check > :global(:not(.t):not(.d)) { grid-column: 1 / -1; }
  .check .btn, .check .link { align-self: flex-start; }
  .login { display: flex; flex-direction: column; gap: 6px; }
  .login .code { align-self: flex-start; font-size: 15px; letter-spacing: 0.08em; color: var(--text); background: var(--surface-hover); border-radius: var(--r-sm); padding: 3px 8px; cursor: pointer; }
  .state { width: 7px; height: 7px; border-radius: 50%; flex: none; background: var(--green); }
  .state.warn { background: var(--amber); }
  .state.missing, .state.invalid { background: var(--red); }
  .state.unknown { background: var(--muted); }
  .check.invalid .d { color: var(--red); }
  .check.unknown .d { color: var(--muted); }
  .creds { margin-top: 12px; }
  .credhead { display: flex; align-items: center; justify-content: space-between; }
  .credhead .link { margin: 0; }
  .btn.small { height: 24px; padding: 0 8px; font-size: 12px; }
  .link.small { font-size: 12px; }
  .git { display: flex; gap: 6px; flex-wrap: wrap; }
  .git input { height: 24px; padding: 0 8px; border-radius: var(--r-sm); background: var(--surface); color: var(--text); border: 1px solid var(--line-strong); font-size: 12px; width: 100%; }
  .note { color: var(--text-3); font-size: 13px; margin: 0; display: flex; flex-wrap: wrap; align-items: center; gap: 8px; }
  .note.failed { color: var(--red); }
  .muted { color: var(--muted); font-size: 13px; margin: 0; }
  .link { color: var(--accent); font-size: 13px; margin-top: 8px; margin-right: 12px; }
  .link:hover { text-decoration: underline; }
</style>
