<!-- Whether a reply kept the docs that cover its code in step: it updated or opened them, Hub asked it to check them, or it did not after asking. Quiet unless Hub had to ask.
     Used by Changes.svelte, Transcript.svelte. -->
<script lang="ts">
  // Whether a reply kept the docs that cover its code in step: it updated or opened them, Hub
  // asked it to check them, or it did not after asking. Quiet unless Hub had to ask.
  import type { DocsState } from "../transcript";
  import Icon from "./Icon.svelte";

  let { docs }: { docs: DocsState } = $props();

  const where = $derived(docs.sections.map((s) => (s.heading ? `${s.doc} › ${s.heading}` : s.doc)).join("\n"));
  const shown = $derived(
    docs.state === "updated" ? { tone: "done", text: "docs updated", title: `Updated the docs about this code:\n${where}` }
    : docs.state === "checked" ? { tone: "done", text: "docs checked", title: `Looked at the docs about this code:\n${where}` }
    : docs.state === "asked" ? { tone: "busy", text: "asked to check docs…", title: `It changed code these docs describe without opening them; Hub asked it to check:\n${where}` }
    : docs.state === "skipped" ? { tone: "none", text: "docs as before", title: "It put back what an earlier turn changed, so the docs need nothing new" }
    : { tone: "none", text: "docs not checked", title: `Hub asked, and it still did not open:\n${where}` },
  );
</script>

<span class="docs {shown.tone}" data-tip={shown.title}>
  {#if shown.tone === "busy"}<span class="pulse"></span>{:else if docs.state === "skipped"}<Icon name="slash" size={11} />{:else if shown.tone === "done"}<Icon name="check" size={11} />{:else}<Icon name="x" size={11} />{/if}{shown.text}
</span>

<style>
  .docs { display: inline-flex; align-items: center; gap: 4px; font-size: 12px; white-space: nowrap; }
  .done { color: var(--muted); }
  .busy { color: var(--amber); }
  .none { color: var(--muted); }
  .pulse { width: 6px; height: 6px; border-radius: 50%; background: var(--amber); }
</style>
