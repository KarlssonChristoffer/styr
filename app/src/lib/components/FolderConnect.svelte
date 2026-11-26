<!-- The box that connects GitHub from the folder picker: the code to type, and each machine's state.
     Used by FolderPicker.svelte. -->
<script lang="ts">
  import { poll } from "../poll";
  import { api } from "../api";
  import { hub } from "../state.svelte";
  import { gh, giveUp as ghGiveUp, look as ghLook, refresh as ghRefresh } from "../github.svelte";
  import { modal } from "../pop";
  import HostChip from "./HostChip.svelte";
  import Icon from "./Icon.svelte";
  import ProviderLogo from "./ProviderLogo.svelte";

  let { machines, waitingOn, stateOf, started, startConnect, ghHosts, onclose }: {
    machines: { name: string; host: string | null }[];
    waitingOn: { name: string; host: string | null } | null;
    stateOf: (host: string | null) => string;
    started: string | null | undefined;
    startConnect: (host: string | null) => void | Promise<void>;
    ghHosts: () => (string | null)[];
    onclose: () => void;
  } = $props();
  const hk = (h: string | null | undefined) => h ?? "";

// While a code waits to be typed, look every two seconds; when a machine is signed in, ask again.
$effect(() => {
  if (!waitingOn) return;
  const host = waitingOn.host;
  const timer = poll(async () => {
    const s = await ghLook(host);
    if (s.state === "ready") void ghRefresh(ghHosts());
  }, 2000);
  return () => timer();
});

// Once the machine the sign-in was started on is signed in, the box closes by itself after a moment.
$effect(() => {
  if (started === undefined || stateOf(started) !== "ready") return;
  const timer = setTimeout(onclose, 1600);
  return () => clearTimeout(timer);
});</script>

  <div class="scrim" role="presentation" in:modal|global out:modal|global={{ out: true }}>
    <div class="dialog" role="dialog" aria-label="Connect GitHub">
      <div class="d-title"><ProviderLogo id="github" size={16} /> Connect GitHub{waitingOn ? ` on ${waitingOn.name}` : ""}</div>
      <p class="muted">Hub starts GitHub's own sign-in on that machine. Nothing is pasted, and no password passes through Hub.</p>
      {#if waitingOn}
        {@const w = gh.machines[hk(waitingOn.host)]}
        <div class="step">1. Open <span class="mono acc">github.com/login/device</span>
          <button class="use" onclick={(e) => { e.preventDefault(); api.openUrl(w?.url ?? "https://github.com/login/device").catch((err) => hub.fail(err)); }}>Open</button>
        </div>
        <div class="step">2. Type this code</div>
        <div class="code mono">{w?.code ?? "…"}</div>
        <div class="muted"><span class="pip"></span> Waiting for you to allow it…
          <button class="link" onclick={(e) => { e.preventDefault(); ghGiveUp(waitingOn!.host); }}>Give up</button>
        </div>
      {/if}
      <div class="ms">
        {#each machines as m (m.name)}
          {@const st = gh.machines[hk(m.host)]}
          <div class="m">
            <HostChip name={m.name} /><span class="grow"></span>
            {#if st?.state === "ready"}<span class="ok"><Icon name="check" size={12} /> {st.login ?? "connected"}</span>
            {:else if st?.state === "waiting"}<span class="amb">waiting</span>
            {:else if st?.state === "missing"}<span class="muted" data-tip={st.message ?? undefined}>no GitHub CLI</span>
            {:else if st?.state === "old"}<span class="muted">needs the next Hub</span>
            {:else}
              {#if st?.state === "failed" && st.message}<span class="muted err" data-tip={st.message}>{st.message}</span>{/if}
              <button class="use" onclick={(e) => { e.preventDefault(); startConnect(m.host); }}>Connect</button>
            {/if}
          </div>
        {/each}
      </div>
      <div class="d-foot"><button class="use" onclick={(e) => { e.preventDefault(); }}>Close</button></div>
    </div>
  </div>

<style>
  .scrim { position: absolute; inset: 0; z-index: 3; background: rgba(20, 19, 18, 0.72); border-radius: var(--r-lg); display: grid; justify-items: center; align-items: safe center; overflow-y: auto; }
  .dialog { width: min(480px, calc(100% - 32px)); background: var(--surface-2); border: 1px solid var(--line-strong); border-radius: var(--r-lg); padding: 16px 18px; display: grid; gap: 8px; box-shadow: 0 12px 32px rgba(0, 0, 0, 0.45); }
  .d-title { display: flex; align-items: center; gap: 8px; font-size: 15px; font-weight: 500; color: var(--text); }
  .dialog p { margin: 0; line-height: 1.45; }
  .step { color: var(--text-2); font-size: 13px; display: flex; align-items: center; gap: 8px; }
  .acc { color: var(--accent); }
  .code { font-size: 28px; letter-spacing: 5px; text-align: center; padding: 12px; border: 1px solid var(--line-strong); border-radius: var(--r-md); background: var(--bg); color: var(--text); user-select: all; }
  .pip { display: inline-block; width: 7px; height: 7px; border-radius: 50%; background: var(--amber); margin-right: 6px; }
  .ms { border-top: 1px solid var(--line); margin-top: 4px; padding-top: 8px; display: grid; gap: 6px; }
  .ms .m { display: flex; align-items: center; gap: 8px; font-size: 13px; color: var(--text-2); }
  .ok { color: var(--green); display: inline-flex; align-items: center; gap: 4px; font-size: 13px; }
  .amb { color: var(--amber); font-size: 13px; }
  .d-foot { display: flex; justify-content: flex-end; }
  .use { display: inline-flex; align-items: center; gap: 4px; font-size: 12px; color: var(--text-3); padding: 2px 8px; border-radius: var(--r-sm); background: var(--surface-hover); flex: none; }
  .use:hover { color: var(--text); }
  .grow { flex: 1; }
  .muted { color: var(--muted); font-size: 12px; }
  .mono { font-family: var(--mono); font-size: 12px; }
  .link { color: var(--muted); text-decoration: underline; margin-left: 10px; font-size: 12px; }
  .link:hover { color: var(--text); }
  .muted.err { color: var(--red); max-width: 200px; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
</style>
