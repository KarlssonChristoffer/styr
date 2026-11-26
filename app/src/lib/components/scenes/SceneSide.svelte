<!-- The column that is the same in every scene: the plan limits, the machines and the update. Only the main column changes with the scene.
     Used by Scenes.svelte. -->
<script lang="ts">
  import Quiet from "../Quiet.svelte";
  // The column that is the same in every scene: the plan limits, the machines
  // and the update. Only the main column changes with the scene.
  import { machineGlyph } from "../../refs";
  import { sim } from "../../sim.svelte";
  import { hub } from "../../state.svelte";
  import Ref from "../Ref.svelte";

  // Simulating swaps in made-up limits that creep up, so the bars show how they look when they move.
  const limits = $derived<{ label: string; percent: number; resets: string | null }[]>(sim.on ? sim.limits : (hub.plan?.limits ?? []));
  const colour = (p: number) => (p >= 90 ? "var(--red)" : p >= 75 ? "var(--amber)" : "var(--accent)");
  const short = (l: string) => l.replace(/^Weekly · all models$/, "Weekly").replace(/^Weekly · /, "Weekly ").replace(/ limit$/, "");

  const rows = $derived(
    hub.hosts.map((h) => {
      const mine = hub.everything.filter((t) => hub.hostOf(t) === h.name);
      return { h, running: mine.filter((t) => t.status === "running").length, asks: mine.filter((t) => t.status === "waiting_on_you").length };
    }),
  );
  const online = $derived(hub.hosts.filter((h) => h.state === "online").length);

  const upd = $derived(hub.update);
</script>

<div class="side">
  <div class="lbl">Limits</div>
  <div class="blk p">
    {#if !limits.length}
      <Quiet text="No limits" />
    {:else}
      <div class="lim">
        {#each limits as l (l.label)}
          <div class="meter">
            <span class="n">{short(l.label)}</span>
            <span class="v">{Math.round(l.percent)}%</span>
            <span class="bar"><i style:width="{Math.min(100, l.percent)}%" style:background={colour(l.percent)}></i></span>
            {#if l.resets}<span class="sub">resets {l.resets}</span>{/if}
          </div>
        {/each}
      </div>
    {/if}
  </div>

  <div class="gap"></div>
  <div class="lbl">Machines <span class="n">{online} of {hub.hosts.length} online</span></div>
  <div class="blk p">
    {#if !rows.length}
      <Quiet text="No machines" />
    {:else}
      <div class="mach">
        {#each rows as { h, running, asks } (h.name)}
          <button class="m" class:off={h.state !== "online"} onclick={() => hub.openMachine(h.name)}>
            <Ref kind={machineGlyph(h.name, h.local)} label={h.name} online={h.state === "online"} />
            <span class="grow"></span>
            {#if h.state !== "online"}<span class="s">{h.state}</span>
            {:else if asks}<span class="s am">{asks} {asks === 1 ? "needs you" : "need you"}</span>
            {:else if running}<span class="s gr">{running} working</span>
            {:else}<span class="s">idle</span>{/if}
          </button>
        {/each}
      </div>
    {/if}
  </div>

  {#if upd && upd.state !== "disabled"}
    <div class="gap"></div>
    <div class="lbl">Update</div>
    <div class="blk p">
      <button class="u" onclick={() => hub.openSettings("updates")} data-tip="Settings > Updates">
        <span>Installed <span class="mono">{upd.version}</span></span>
        <span class="s" class:gr={upd.state === "idle"} class:am={upd.state === "ready" || upd.state === "failed"}>
          {upd.state === "building" ? `building${upd.step ? ` · ${upd.step}` : ""}` : upd.state === "ready" ? "ready to install" : upd.state === "failed" ? "failed" : upd.state === "checking" ? "checking" : upd.state === "restarting" ? "restarting" : "up to date"}
        </span>
      </button>
    </div>
  {/if}
</div>

<style>
  .lim { display: flex; flex-direction: column; gap: 12px; font-size: 13px; color: var(--text-2); }
  .meter { display: grid; grid-template-columns: minmax(0, 1fr) auto; row-gap: 6px; align-items: baseline; }
  .meter .v { color: var(--text); font-variant-numeric: tabular-nums; }
  .meter .bar { grid-column: 1 / 3; height: 6px; border-radius: 3px; background: color-mix(in srgb, var(--accent) 14%, var(--bg)); overflow: hidden; }
  .meter .bar i { display: block; height: 100%; border-radius: 3px; transition: width 0.6s var(--ease); }
  .meter .sub { grid-column: 1 / 3; margin-top: -2px; color: var(--muted); font-size: 11px; }
  .mach { display: flex; flex-direction: column; gap: 3px; }
  .m { display: flex; align-items: center; gap: 10px; width: 100%; min-width: 0; height: 24px; text-align: left; padding: 0; font-size: 13px; color: var(--text-2); }
  .m.off { opacity: 0.6; }
  .grow { flex: 1; }
  .s { color: var(--muted); font-size: 12px; white-space: nowrap; }
  .s.am { color: var(--amber); }
  .s.gr { color: var(--green); }
  .u { display: flex; align-items: center; justify-content: space-between; gap: 10px; width: 100%; padding: 0; font-size: 13px; color: var(--text-2); text-align: left; }
</style>
