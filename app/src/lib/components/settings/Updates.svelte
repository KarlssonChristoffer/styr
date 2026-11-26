<!-- Settings page: updates. Used by Settings.svelte. -->
<script lang="ts">
  import { quiet } from "../../quiet";
  import { sc, edit } from "./core.svelte";
  import { api } from "../../api";
  import type { DiskUsage } from "../../bindings/DiskUsage";
  import type { Cleanup } from "../../bindings/Cleanup";
  import { ago, bytes, shortVersion } from "../../format";
  import { hub } from "../../state.svelte";
  import Field from "../Field.svelte";
  import Switch from "../Switch.svelte";
  import BuildWaiting from "../BuildWaiting.svelte";
  import { hubdHas } from "../../compat";
  import UpdateCenter from "../UpdateCenter.svelte";

  const about = $derived(sc.about);
  const saved = $derived(sc.core?.saved);

  let buildModes = $state(false);

  quiet(hubdHas("update.modes").then((v) => (buildModes = v)), "hubd has"); // compat-until: 2026-12-03

  const BUILD_MODES: [string, string][] = [["right_away", "Right away"], ["merges", "After merges"], ["times", "At times"], ["manual", "Only when I say"]];

  const buildMode = $derived(saved?.update_build_mode || "right_away");

  const clockMins = (mins: number) => (mins >= 60 ? `${mins / 60} h` : `${mins} min`);

  async function setBuildTimes(v: string) {
    await edit("update.build_utc_offset_min", -new Date().getTimezoneOffset());
    await edit("update.build_times", v);
  }

  let disk = $state<DiskUsage | null>(null);

  let cleaning = $state(false);

  let cleanup = $state<Cleanup | null>(null);

  let sweeping = $state(false);

  async function cleanUp() {
    sweeping = true;
    try {
      cleanup = await api.cleanUp();
    } catch (e) {
      hub.fail(e);
    } finally {
      sweeping = false;
    }
  }

  function tally(c: Cleanup): string {
    const kinds: [string, string, string][] = [["worktree", "worktree", "worktrees"], ["branch", "branch", "branches"], ["temp", "temporary file", "temporary files"], ["log", "empty log", "empty logs"]];
    const parts = kinds
      .map(([k, one, many]) => {
        const n = c.removed.filter((r) => r.kind === k).reduce((sum, r) => sum + r.count, 0);
        return n ? `${n} ${n === 1 ? one : many}` : "";
      })
      .filter(Boolean);
    return parts.length ? parts.join(", ") : "nothing";
  }

  async function cleanBuild() {
    cleaning = true;
    try {
      disk = await api.freeDisk();
    } catch (e) {
      hub.fail(e);
    } finally {
      cleaning = false;
    }
  }

  const updateLabel: Record<string, string> = {
    idle: "Up to date",
    checking: "Checking…",
    building: "Building in the background…",
    ready: "New version ready",
    restarting: "Restarting…",
    failed: "The last update failed",
  };

  $effect(() => {
    if (hub.connected) quiet(api.disk().then((d) => (disk = d)), "disk");
  });
  $effect(() => {
    if (hub.connected) quiet(api.cleanup().then((c) => (cleanup = c)), "cleanup");
  });
</script>

<h1>Updates</h1>
<UpdateCenter>
  {#snippet settings()}
    <section>
      <div class="line">
        <div class="what">
          <div class="name">Version <span class="mono">{hub.update?.version ?? about?.version ?? "…"}</span>{#if hub.update && about?.update_enabled} <span class="pill" class:warn={hub.update.state === "failed"}>{updateLabel[hub.update.state] ?? hub.update.state}</span>{/if}</div>
          <div class="hint">
            {#if about && !about.update_enabled}
              A development build. It does not update itself.
            {:else if about}
              Follows <span class="mono">{about.remote}/{about.branch}</span> and checks every {Math.round(Number(about.interval_secs) / 60)} minutes. New versions are built and tested in the background; Install only switches over.
            {/if}
          </div>
        </div>
      </div>
      {#if hub.update?.message}<pre class="message">{hub.update.message}</pre>{/if}
      {#if hub.update?.step || hub.update?.building}
        <div class="foot">
          {hub.update.building ? `Building the newer ${shortVersion(hub.update.building)} in the background; you can install the ready one meanwhile.` : ""}
          {hub.update.step ?? ""}
        </div>
      {/if}
      {#if hub.update?.checked_at}<div class="foot">Last checked {ago(hub.update.checked_at)}{hub.update.target ? ` · next ${shortVersion(hub.update.target)}` : ""}</div>{/if}
      {#if saved && about?.version !== "dev"}
        <div class="line">
          <div class="what"><div class="name">Update automatically</div></div>
          <div class="control"><Switch label="Update automatically" on={saved.update_enabled} onchange={(v) => edit("update.enabled", v)} /></div>
        </div>
        <div class="line">
          <div class="what"><div class="name">Ask before installing</div></div>
          <div class="control"><Switch label="Ask before installing" on={saved.update_ask} onchange={(v) => edit("update.ask", v ? null : false)} /></div>
        </div>
        <div class="line">
          <div class="what">
            <div class="name">Rust builds</div>
            <div class="hint">
              {#if !buildModes}Needs the next version of Hub on this machine.{:else}Changes to the programs: a full build, about 10 minutes. Page changes always build at once.{/if}
            </div>
          </div>
          <div class="control seg">
            {#each BUILD_MODES as [id, label] (id)}
              <button class:active={buildMode === id} disabled={!buildModes} onclick={() => edit("update.build_mode", id === "right_away" ? null : id)}>{label}</button>
            {/each}
          </div>
        </div>
        {#if buildModes && buildMode === "merges"}
          <div class="line sub">
            <div class="what"><div class="hint">Build at this many Rust changes. Notice at {(saved.update_build_notice_at ?? []).join(" and ") || "none"}.</div></div>
            <div class="control seg">
              {#each [3, 5, 8, 10, 12] as n (n)}
                <button class:active={saved.update_build_merges === n} onclick={() => edit("update.build_merges", n === 8 ? null : n)}>{n}</button>
              {/each}
            </div>
          </div>
          <div class="line sub">
            <div class="what"><div class="hint">Or after this long.</div></div>
            <div class="control seg">
              {#each [30, 60, 120, 240] as m (m)}
                <button class:active={Number(saved.update_build_max_wait_mins) === m} onclick={() => edit("update.build_max_wait_mins", m === 60 ? null : m)}>{clockMins(m)}</button>
              {/each}
            </div>
          </div>
        {:else if buildModes && buildMode === "times"}
          <div class="line sub">
            <div class="what"><div class="hint">Build at these times.</div></div>
            <div class="control"><Field value={(saved.update_build_times ?? []).join(", ")} placeholder="16:30, 22:30" width={180} onsave={setBuildTimes} /></div>
          </div>
        {/if}
        <div class="line">
          <div class="what"><div class="name">Branch</div></div>
          <div class="control"><Field value={saved.update_branch} placeholder="main" width={160} onsave={(v) => edit("update.branch", v)} /></div>
        </div>
        <div class="line">
          <div class="what"><div class="name">Check every</div></div>
          <div class="control seg">
            {#each [[60, "1 min"], [180, "3 min"], [600, "10 min"], [3600, "1 h"]] as [secs, label] (secs)}
              <button class:active={Number(saved.update_interval_secs) === secs} onclick={() => edit("update.interval_secs", secs === 180 ? null : secs)}>{label}</button>
            {/each}
          </div>
        </div>
      {/if}
    </section>

    <BuildWaiting />

    <h2 class="with-action">
      Disk
      {#if hub.connected}<button class="link" disabled={cleaning} onclick={cleanBuild}>{cleaning ? "Cleaning…" : "Clean build folder"}</button>{/if}
    </h2>
    <section>
      {#each disk?.items.filter((i) => i.bytes > 0) ?? [] as item (item.path)}
        <div class="change" data-tip={item.path}>
          <span class="tool-name">{item.label}</span>
          <span class="subject"><span class="dim mono ellipsis">{item.path}</span></span>
          <span class="mono">{bytes(item.bytes)}</span>
        </div>
      {:else}
        <div class="empty">{disk ? "Nothing yet." : "Measuring…"}</div>
      {/each}
      {#if disk}
        <div class="foot">
          {disk.data_free != null ? `${bytes(disk.data_free)} free where Hub keeps its data` : ""}{disk.build_free != null && disk.build_free !== disk.data_free ? ` · ${bytes(disk.build_free)} free for builds` : ""}
        </div>
      {/if}
      {#if saved && about?.version !== "dev"}
        <div class="line">
          <div class="what"><div class="name">Build folder</div><div class="hint">Where new versions are built. Needs 8 GB free.</div></div>
          <div class="control"><Field value={saved.update_target_dir ?? ""} placeholder="src\target" width={220} onsave={(v) => edit("update.target_dir", v.trim() || null)} /></div>
        </div>
      {/if}
    </section>

    <h2 class="with-action">
      After agent work
      {#if hub.connected}<button class="link" disabled={sweeping} onclick={cleanUp}>{sweeping ? "Cleaning up…" : "Clean up now"}</button>{/if}
    </h2>
    <section>
      <div class="line">
        <div class="what"><div class="name">Clean up by itself</div><div class="hint">Removes pushed worktrees and branches threads left. Unpushed work stays.</div></div>
      </div>
      {#if cleanup}
        <div class="foot">{ago(cleanup.at) === "now" ? "Just now" : `${ago(cleanup.at)} ago`}: removed {tally(cleanup)}{cleanup.freed > 0 ? ` (${bytes(cleanup.freed)})` : ""}.</div>
        {#each cleanup.kept as k (k.kind + k.path)}
          <div class="change" data-tip={k.path}>
            <span class="tool-name">Kept {k.kind}</span>
            <span class="subject"><span class="dim mono ellipsis">{k.path}</span></span>
            <span class="dim">{k.why}</span>
          </div>
        {/each}
      {:else}
        <div class="foot">Has not run yet; it first runs a few minutes after Hub starts.</div>
      {/if}
    </section>

  {/snippet}
</UpdateCenter>

<style>
  /* @kit settings */
</style>
