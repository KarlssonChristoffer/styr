<!-- The devlog: the entry Hub writes about each day you worked, in the night after it: the date in grey, the first line beside it, the rest when clicked. A day's date shows it in the Day card.
     Used by CardBody.svelte. -->
<script lang="ts">
  // The devlog: the entry Hub writes about each day you worked, in the night after it: the date
  // in grey, the first line beside it, the rest when clicked. A day's date shows it in the Day card.
  import { api } from "../api";
  import type { DevlogDay } from "../bindings/DevlogDay";
  import { recall, remember } from "../lastSeen";
  import { quiet } from "../quiet";
  import Markdown from "./Markdown.svelte";
  import Quiet from "./Quiet.svelte";

  let { show }: { show: (date: string) => void } = $props();

  let devlog = $state<DevlogDay[] | null>(recall<DevlogDay[]>("devlog"));
  let open = $state<string | null>(null);

  quiet(api.devlog(30), "devlog").then((d) => {
    if (!d) return;
    devlog = d;
    remember("devlog", d);
  });

  /** The first line of a day's entry, without Markdown, for the closed list. */
  const gist = (text: string) =>
    (text.split("\n").find((l) => l.trim() && !l.trim().startsWith("#")) ?? text).replace(/[*_`#>]/g, "").replace(/^\s*[-•]\s*/, "").trim();
  /** The rest of the entry, under the gist. Older entries are one paragraph, all gist. */
  const entryBody = (text: string) => {
    const lines = text.trim().split("\n");
    const first = lines.findIndex((l) => l.trim() && !l.trim().startsWith("#"));
    return lines.slice(first + 1).join("\n").trim();
  };

  const day = (date: string) => new Date(date + "T12:00").toLocaleDateString([], { weekday: "short", day: "numeric", month: "short" });
</script>

{#each devlog ?? [] as d (d.date)}
  <div class="entry" class:open={open === d.date}>
    <button class="day" onclick={() => show(d.date)} data-tip="Show the day">{day(d.date)}</button>
    <div class="summary-col">
      <button class="summary" onclick={() => (open = open === d.date ? null : d.date)}>{gist(d.summary)}</button>
      {#if open === d.date && entryBody(d.summary)}<div class="entry-text"><Markdown text={entryBody(d.summary)} rich /></div>{/if}
    </div>
  </div>
{:else}
  <Quiet kind={devlog ? "none" : "loading"} text={devlog ? "Nothing yet" : undefined} />
{/each}

<style>
  .entry { display: flex; gap: 14px; padding: 4px 0; }
  .entry:first-child { padding-top: 0; }
  .day { flex: none; width: 84px; text-align: left; color: var(--muted); font-size: 13px; padding-top: 1px; font-variant-numeric: tabular-nums; }
  .day:hover { color: var(--accent); }
  .summary { flex: 1; min-width: 0; text-align: left; color: var(--text-2); overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .entry.open .summary { white-space: normal; }
  .summary-col { flex: 1; min-width: 0; display: flex; flex-direction: column; }
  .summary-col .summary { width: 100%; }
  .entry-text { max-width: var(--prose); color: var(--text-2); padding: 4px 0 6px; }
</style>
