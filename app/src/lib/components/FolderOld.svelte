<!-- A machine whose Hub is too old to list its folders: what its updater is doing, and the buttons that
     get it going. Used by FolderPicker.svelte. -->
<script lang="ts">
  import { quiet } from "../quiet";
  import { poll } from "../poll";
  import { api } from "../api";
  import type { UpdateStatus } from "../bindings/UpdateStatus";
  import { shortVersion } from "../format";
  import { hub } from "../state.svelte";

  type Old = { theirs: string | null; ours: string | null; update: UpdateStatus | null };
  let { old = $bindable(), target, open, onclose, onnewer }: {
    old: Old | null;
    target: string | null;
    open: boolean;
    /** The picker should close: the update needs an answer in Settings. */
    onclose: () => void;
    /** The machine runs a newer Hub now: list its folders again. */
    onnewer: () => void;
  } = $props();
  let updating = $state(false);

  // While the note shows, look again every few seconds: once the machine runs a newer hubd, list
  // its folders.
  $effect(() => {
    if (!old || !open || !target) return;
    const name = target, was = old.theirs;
    const timer = poll(async () => {
      const now = await quiet(api.version(name), "version", null);
      if (!now || target !== name || !old) return;
      if (now.version !== was) {
        old = null;
        updating = false;
        onnewer();
      } else old = { ...old, update: now };
    }, 5000);
    return () => timer();
  });

  // Installed before the PC sent it the source: its updater still asks GitHub.
  const noLogin = $derived(/git fetch|could not read Username|Authentication failed/i.test(old?.update?.message ?? ""));
  /** What the machine's own updater is doing, in words. */
  const oldStatus = $derived.by(() => {
    const u = old?.update;
    if (updating) return `Updating ${target}. Its threads carry on after the restart, and this lists its folders once it is back.`;
    if (!u) return `Its version could not be read, so it may not update by itself.`;
    const newer = u.target ? ` ${shortVersion(u.target)}` : "";
    switch (u.state) {
      case "checking":
      case "building":
        return `It is building${newer || " a newer version"} now, which takes a few minutes, and switches once no thread there is working.`;
      case "ready":
        return `${newer ? `Version${newer}` : "A newer version"} is ready there and waits to be installed.`;
      case "restarting":
        return `It is restarting into${newer || " a newer version"} now.`;
      case "failed":
        if (noLogin) return `It tried to get the new version from GitHub, where it has no login. Update now sends it from this PC instead.`;
        return `Its last update failed${u.message ? `: ${u.message}` : "."}`;
      default:
        return `It does not have a newer version to switch to by itself.`;
    }
  });

  async function act(how: "install" | "retry" | "reinstall") {
    if (!target) return;
    const name = target;
    updating = true;
    try {
      if (how === "install") await api.installUpdate(name);
      else if (how === "retry") await api.retryUpdate(name);
      else {
        const reply = await api.updateHost(name);
        // A sudo password or whether to build here: answered once, in Settings.
        if (reply.sudo || reply.build) {
          updating = false;
          onclose();
          hub.openSettings("machines");
        }
      }
    } catch (e) {
      updating = false;
      hub.fail(e);
    }
  }
</script>

{#if old}
  <div class="old">
    <p>
      <b>{target}</b> runs Hub <span class="mono">{shortVersion(old.theirs)}</span>, which cannot list folders. This PC runs
      <span class="mono">{shortVersion(old.ours)}</span>.
    </p>
    <p class="muted">{oldStatus}</p>
    {#if !updating}
      <div class="acts">
        {#if old.update?.state === "ready"}
          <button class="use" onclick={(e) => { e.preventDefault(); act("install"); }}>Install on {target}</button>
        {:else if old.update?.state === "failed" && !noLogin}
          <button class="use" onclick={(e) => { e.preventDefault(); act("retry"); }}>Try again</button>
        {/if}
        {#if !old.update || !["checking", "building", "ready", "restarting"].includes(old.update.state)}
          <button class="use" data-tip="Sends {target} this PC's version over SSH, so it needs no GitHub login there, and has it switch once no thread there is working" onclick={(e) => { e.preventDefault(); act("reinstall"); }}>
            Update {target} now
          </button>
        {/if}
      </div>
    {/if}
    <p class="muted">Until then, type the full path, e.g. <span class="mono">~/srv/game</span>.</p>
  </div>
{/if}

<style>
  .old { margin: 6px 8px; font-size: 13px; color: var(--text-2); display: grid; gap: 6px; }
  .old p { margin: 0; }
  .old b { font-weight: 500; color: var(--text); }
  .acts { display: flex; gap: 6px; }
  .use { display: inline-flex; align-items: center; gap: 4px; font-size: 12px; color: var(--text-3); padding: 2px 8px; border-radius: var(--r-sm); background: var(--surface-hover); flex: none; }
  .use:hover { color: var(--text); }
  .muted { color: var(--muted); font-size: 12px; }
  .mono { font-family: var(--mono); font-size: 12px; }
</style>
