<!-- För: agent. The note where an edit cut the conversation, with what it cut once opened (Transcript). -->
<script lang="ts">
  import type { Item as LogItem } from "../transcript";
  import { plural, when } from "../transcriptWords";
  import Icon from "./Icon.svelte";
  import StreamingText from "./StreamingText.svelte";

  let { item }: { item: Extract<LogItem, { kind: "rewound" }> } = $props();
  /** What an edit left out is only drawn once its line is opened. */
  let opened = $state(false);
  const said = $derived(item.cut.filter((x) => x.kind === "user" && !x.fromHub && !x.review && !x.designReview));
</script>

<details class="note rewound" ontoggle={(e) => (opened = (e.currentTarget as HTMLDetailsElement).open)}>
  <summary>
    <Icon name="pen" size={13} />
    <span>You edited from here {when(item.ts)}{item.files ? " and rolled the files back" : ""}. {said.length ? `${plural(said.length, "message")} and what followed ${said.length === 1 ? "was" : "were"} left out.` : "What followed was left out."}</span>
    <span class="chev"><Icon name="chevron" size={12} /></span>
  </summary>
  {#if opened}
  <div class="cut">
    {#each item.cut as x, j (j)}
      {#if x.kind === "user" && !x.fromHub && !x.review && x.text}
        <div class="cut-user"><div class="bubble">{x.text}</div></div>
      {:else if x.kind === "assistant" && x.text.trim()}
        <div class="cut-agent"><StreamingText rich text={x.text} streaming={false} /></div>
      {:else if x.kind === "rewound"}
        <div class="cut-note">An earlier edit was here too.</div>
      {/if}
    {/each}
  </div>
  {/if}
</details>
<style>
  /* @kit transcript */
</style>
