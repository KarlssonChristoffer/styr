<!-- Records and badges over the last year: your biggest days, and small badges for habits, earned or still to earn with how far along you are. All counted from the Pulse.
     Used by CardBody.svelte. -->
<script lang="ts">
  import Quiet from "./Quiet.svelte";
  // Records and badges over the last year: your biggest days, and small badges for habits,
  // earned or still to earn with how far along you are. All counted from the Pulse.
  import { dateKey, lines, loadLines, loadPulse, pulse } from "../overview.svelte";
  import HowCounted from "./HowCounted.svelte";
  import { hub } from "../state.svelte";

  loadPulse();
  loadLines();

  let all = $state(false);

  type Badge = { glyph: string; name: string; what: string; have: number; need: number };
  type Record = { label: string; value: string; when?: string; open?: () => void };

  const day = (key: string) => new Date(key + "T12:00").toLocaleDateString([], { day: "numeric", month: "short" });
  const num = (n: number) => n.toLocaleString();

  const found = $derived.by(() => {
    const pd = pulse.data;
    if (!pd) return null;
    // Lines come from git and may still be on their way; the rest counts without them.
    const p = { ...pd, lines: lines.data?.days ?? [] };
    // Messages per day, per hour, per weekday part.
    const perDay = new Map<string, number>();
    const hoursPerDay = new Map<string, Set<number>>();
    let night = 0;
    let early = 0;
    for (const ts of p.moments) {
      const d = new Date(ts);
      const key = dateKey(d);
      perDay.set(key, (perDay.get(key) ?? 0) + 1);
      if (!hoursPerDay.has(key)) hoursPerDay.set(key, new Set());
      hoursPerDay.get(key)!.add(d.getHours());
      if (d.getHours() < 5) night++;
      else if (d.getHours() < 7) early++;
    }
    const active = new Set([...perDay.keys(), ...p.lines.map((l) => l.date)]);
    const weekend = [...active].filter((k) => [0, 6].includes(new Date(k + "T12:00").getDay())).length;

    // Streaks over the active days.
    let longest = 0;
    let run = 0;
    let current = 0;
    const today = new Date();
    for (let i = 400; i >= 0; i--) {
      const key = dateKey(new Date(today.getFullYear(), today.getMonth(), today.getDate() - i));
      run = active.has(key) ? run + 1 : 0;
      longest = Math.max(longest, run);
      if (i === 0 || i === 1) current = Math.max(current, run);
    }

    const best = <T,>(list: T[], by: (t: T) => number) => list.reduce<T | null>((a, b) => (!a || by(b) > by(a) ? b : a), null);
    const msgDay = best([...perDay], ([, n]) => n);
    const hourDay = best([...hoursPerDay], ([, h]) => h.size);
    const commitDay = best(p.lines, (l) => l.commits);
    const addDay = best(p.lines, (l) => l.added);
    const cleanDay = best(p.lines.filter((l) => l.removed > l.added), (l) => l.removed);
    const pullDay = best(p.pulls, (d) => d.count);
    const turns = p.longest_thread?.turns ?? 0;

    const badges: Badge[] = [
      { glyph: "☾", name: "Night owl", what: "50 messages between midnight and five", have: night, need: 50 },
      { glyph: "☀", name: "Early bird", what: "30 messages between five and seven", have: early, need: 30 },
      { glyph: "↯", name: "On a roll", what: "Seven days in a row", have: longest, need: 7 },
      { glyph: "∞", name: "A month in a row", what: "Thirty days in a row", have: longest, need: 30 },
      { glyph: "⚑", name: "Weekend warrior", what: "Worked 10 weekend days", have: weekend, need: 10 },
      { glyph: "⇡", name: "PR machine", what: "Open 10 pull requests in one day", have: pullDay?.count ?? 0, need: 10 },
      { glyph: "#", name: "Hundred", what: "100 commits in one day", have: commitDay?.commits ?? 0, need: 100 },
      { glyph: "+", name: "Big build", what: "Add 10,000 lines in one day", have: addDay?.added ?? 0, need: 10_000 },
      { glyph: "⌫", name: "Clean-up day", what: "Remove 1,000 more lines than you add, in a day", have: cleanDay ? cleanDay.removed - cleanDay.added : 0, need: 1000 },
      { glyph: "◷", name: "Marathon", what: "Write in 12 different hours of one day", have: hourDay?.[1].size ?? 0, need: 12 },
      { glyph: "“", name: "Chatterbox", what: "200 messages in one day", have: msgDay?.[1] ?? 0, need: 200 },
      { glyph: "↻", name: "Long haul", what: "One thread with 100 finished turns", have: turns, need: 100 },
    ];
    badges.sort((a, b) => Number(b.have >= b.need) - Number(a.have >= a.need) || b.have / b.need - a.have / a.need);

    const records: Record[] = [];
    if (longest) records.push({ label: "Longest streak", value: `${longest} ${longest === 1 ? "day" : "days"}`, when: current ? `${current} now` : undefined });
    if (msgDay) records.push({ label: "Most messages", value: `${num(msgDay[1])}`, when: day(msgDay[0]) });
    if (commitDay?.commits) records.push({ label: "Most commits", value: num(commitDay.commits), when: day(commitDay.date) });
    if (addDay?.added) records.push({ label: "Most lines added", value: `+${num(addDay.added)}`, when: day(addDay.date) });
    if (pullDay?.count) records.push({ label: "Most pull requests", value: num(pullDay.count), when: day(pullDay.date) });
    const long = p.longest_thread;
    if (long)
      records.push({
        label: "Longest thread",
        value: `${num(long.turns)} turns`,
        when: long.title,
        open: hub.threads.some((t) => t.id === long.id) ? () => hub.open(long.id) : undefined,
      });
    return { badges, records, earned: badges.filter((b) => b.have >= b.need).length };
  });
</script>

<HowCounted card="records">
  <p>Your best days over the last year. Hover a badge for its goal.</p>
</HowCounted>

{#if !found}
  {#if pulse.loaded}<p class="quiet">{`Could not read: ${pulse.error}`}</p>{:else}<Quiet kind="loading" />{/if}
{:else}
  <div class="badges">
    {#each all ? found.badges : found.badges.slice(0, 4) as b (b.name)}
      {@const done = b.have >= b.need}
      <div class="badge" class:locked={!done} data-tip={`${b.what}. ${done ? "Earned." : `${num(b.have)} of ${num(b.need)}.`}`}>
        <span class="glyph">{b.glyph}</span>
        <span class="text">
          <span class="name">{b.name}</span>
          {#if !done}<span class="bar"><span style:width={`${Math.min(100, (b.have / b.need) * 100)}%`}></span></span>{/if}
        </span>
      </div>
    {/each}
  </div>
  <div class="records">
    {#each all ? found.records : found.records.slice(0, 4) as r (r.label)}
      <div class="record">
        <span class="label">{r.label}</span>
        {#if r.when}
          {#if r.open}<button class="when link" onclick={r.open}>{r.when}</button>{:else}<span class="when">{r.when}</span>{/if}
        {/if}
        <span class="value">{r.value}</span>
      </div>
    {/each}
  </div>
  <p class="caption"><em>{found.earned} of {found.badges.length}</em> badges earned · <button class="more" onclick={() => (all = !all)}>{all ? "show less" : "show all"}</button></p>
{/if}

<style>
  .badges { display: grid; grid-template-columns: repeat(2, minmax(0, 1fr)); gap: 6px; }
  .badge { display: flex; gap: 10px; align-items: center; padding: 6px 8px; border-radius: var(--r-md); background: var(--surface-2); min-width: 0; }
  .glyph { flex: none; width: 26px; height: 26px; border-radius: var(--r-md); display: grid; place-items: center; background: var(--accent-bg); color: var(--accent); font-size: 14px; }
  .badge.locked .glyph { background: var(--surface-hover); color: var(--muted); }
  .text { display: flex; flex-direction: column; min-width: 0; flex: 1; gap: 3px; }
  .name { color: var(--text); font-size: 13px; }
  .badge.locked .name { color: var(--text-3); }
  .bar { height: 4px; border-radius: 2px; background: var(--surface-hover); overflow: hidden; }
  .bar span { display: block; height: 100%; background: color-mix(in srgb, var(--accent) 55%, transparent); }
  .more { color: var(--muted); font-size: 12px; padding: 0; }
  .more:hover { color: var(--text); }
  .records { margin-top: 8px; }
  .record { display: flex; gap: 10px; align-items: baseline; padding: 4px 0; border-top: 1px solid var(--line); font-size: 13px; }
  .label { color: var(--text-3); flex: none; }
  .when { color: var(--muted); font-size: 12px; margin-left: auto; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; min-width: 0; text-align: right; }
  .link:hover { color: var(--accent); }
  .value { color: var(--text); font-variant-numeric: tabular-nums; flex: none; }
  .record .label + .value { margin-left: auto; }
</style>
