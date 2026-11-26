<!-- För: agent. The agent searching the web: one row with the globe, its searches and the pages under it (Transcript). -->
<script lang="ts">
  import { duration, running as isRunning } from "../activity";
  import { hub } from "../state.svelte";
  import { openUrl } from "../native";
  import type { Activity } from "../activity";
  import type { ToolItem } from "../transcript";
  import { siteTint, webLine, webPages } from "../web";
  import { plural } from "../transcriptWords";
  import Icon from "./Icon.svelte";
  import Working from "./Working.svelte";

  let {
    item,
    last,
    live,
    tick,
    inner = false,
    opened,
    ontoggle,
  }: {
    item: Activity;
    /** It is the last row of the thread. */
    last: boolean;
    live: boolean;
    /** The clock, ticking while the thread works. */
    tick: number;
    inner?: boolean;
    opened: boolean;
    ontoggle: () => void;
  } = $props();

  const tools = $derived(item.steps.filter((s): s is ToolItem => s.kind === "tool"));
  const now = $derived(live && last && tools.some(isRunning));
  const pages = $derived(webPages(tools));
  const from = $derived(tools.find((t) => t.since)?.since);
  const to = $derived(now ? tick : Math.max(0, ...tools.map((t) => t.until ?? 0)));
</script>

<!-- The agent searching the web: one row with the globe, its searches and the pages under it. -->
<div class="activity web" class:now class:inner>
  <button class="line" onclick={() => ontoggle()} aria-expanded={opened}>
    <span class="globe">{#if now}<Working size={13} />{:else}<Icon name="globe" size={13} />{/if}</span>
    <span class="text">{now ? "Searching the web…" : "Searched the web"}{pages.length ? ` · ${plural(pages.length, "page")}` : ""}{from && to > from ? ` · ${duration(to - from)}` : ""}</span>
    <span class="chev" class:open={opened}><Icon name="chevron" size={12} /></span>
  </button>
  {#if opened}
    <div class="steps web-steps">
      {#each tools as t (t.id)}
        {@const one = webLine(t)}
        {#if one.query}<div class="query">“{one.query}”</div>{/if}
        {#if one.page}
          <button class="page" data-tip={one.page.url} onclick={() => openUrl(one.page!.url).catch((e) => hub.fail(e))}>
            <span class="site" style:background={siteTint(one.page.host)}>{one.page.host[0]?.toUpperCase()}</span><span class="grow">{one.page.title ?? one.page.url.replace(/^https?:\/\/(www\.)?/, "")}</span><span class="host mono">{one.page.host}</span>
          </button>
        {/if}
      {/each}
      {#each pages.filter((p) => !tools.some((t) => webLine(t).page?.host === p.host)).slice(0, 6) as p (p.url)}
        <button class="page" data-tip={p.url} onclick={() => openUrl(p.url).catch((e) => hub.fail(e))}>
          <span class="site" style:background={siteTint(p.host)}>{p.host[0]?.toUpperCase()}</span><span class="grow">{p.title ?? p.host}</span><span class="host mono">{p.host}</span>
        </button>
      {/each}
    </div>
  {/if}
</div>
<style>
  /* @kit transcript */
</style>
