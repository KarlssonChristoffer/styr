<!-- Limit: the plan window that runs out first, as a ring that empties while you watch, with the time to its reset counting down. One line says how far you are and where the pace leads. The numbers are the limits the core already fetches (`hub.plan`); only the clock is new.
     Used by CardBody.svelte. -->
<script lang="ts">
  import { poll } from "../poll";
  import Quiet from "./Quiet.svelte";
  // Limit: the plan window that runs out first, as a ring that empties while you watch, with the
  // time to its reset counting down. One line says how far you are and where the pace leads.
  // The numbers are the limits the core already fetches (`hub.plan`); only the clock is new.
  import { countdown, resetMoment, windowMs } from "../format";
  import { hub } from "../state.svelte";
  import HowCounted from "./HowCounted.svelte";

  let now = $state(Date.now());
  $effect(() => {
    const t = poll(() => (now = Date.now()), 1000);
    return () => t();
  });

  const limits = $derived(hub.plan?.limits ?? []);
  /** The window that would run out first: the fullest one. */
  const main = $derived([...limits].sort((a, b) => b.percent - a.percent)[0] ?? null);
  const rest = $derived(limits.filter((l) => l !== main));
  const reset = $derived(main ? resetMoment(main) : null);
  const left = $derived(reset ? reset - now : null);
  const span = $derived(main ? windowMs(main.label) : null);
  /** Where this pace leads by the reset, once enough of the window has passed to say. */
  const pace = $derived.by(() => {
    if (!main || !reset || !span) return null;
    const done = (now - (reset - span)) / span;
    return done >= 0.1 && done <= 1 ? Math.round(main.percent / done) : null;
  });
  const hm = $derived.by(() => {
    if (left === null) return "";
    const c = countdown(left);
    return c.d ? `${c.d} d ${c.h} h` : c.h ? `${c.h} h ${String(c.m).padStart(2, "0")}` : `${c.m} min ${String(c.s).padStart(2, "0")} s`;
  });
  const colour = (p: number) => (p >= 90 ? "var(--red)" : p >= 75 ? "var(--amber)" : "var(--green)");
  const R = 38;
  const C = 2 * Math.PI * R;
</script>

<HowCounted card="limit">
  <p>Plan limit left in the fullest window.</p>
</HowCounted>

{#if !main}
  <Quiet text="No limits" />
{:else}
  <div class="top">
    <svg viewBox="0 0 100 100" width="96" height="96" aria-hidden="true">
      <circle cx="50" cy="50" r={R} fill="none" stroke="var(--line-strong)" stroke-width="7" />
      <circle cx="50" cy="50" r={R} fill="none" stroke={colour(main.percent)} stroke-width="7" stroke-linecap="round" stroke-dasharray="{(C * (100 - main.percent)) / 100} {C}" transform="rotate(-90 50 50)" />
    </svg>
    <div class="say">
      <div class="figure">{hm || `${100 - main.percent}% left`}</div>
      <div class="sub">{main.label}{reset ? ` · resets ${new Date(reset).toLocaleTimeString([], { hour: "2-digit", minute: "2-digit", hourCycle: "h23" })}` : ""}</div>
    </div>
  </div>
  <p class="caption"><em>{main.percent}%</em> used{pace !== null ? ` · at this pace ${Math.min(pace, 999)}% by the reset` : ""}{rest.length ? ` · ${rest.map((l) => `${l.label.replace(/^Weekly · /, "Week ")}: ${l.percent}%`).join(" · ")}` : ""}</p>
{/if}

<style>
  .top { display: flex; align-items: center; gap: 16px; }
  .say { min-width: 0; }
  .sub { color: var(--muted); font-size: 12px; margin-top: 4px; white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
</style>
