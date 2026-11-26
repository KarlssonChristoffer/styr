<!-- A Markdown file read as a document, the way the docs tab draws it: front matter as small labels, headings in steps, highlighted code with a copy button, `[[links]]` and tables. Used where a file shows outside the docs tab, like a file a reply wrote.
     Used by FilePreview.svelte, NotebookView.svelte. -->
<script lang="ts">
  // A Markdown file read as a document, the way the docs tab draws it: front matter as small
  // labels, headings in steps, highlighted code with a copy button, `[[links]]` and tables. Used
  // where a file shows outside the docs tab, like a file a reply wrote.
  import { renderDoc, splitTarget } from "../docs";
  import { frontMatter } from "../filePreview";
  import { copy } from "../menu.svelte";
  import { openUrl } from "../native";
  import { hub } from "../state.svelte";

  let {
    text,
    rel = "",
    ondoc,
  }: {
    text: string;
    /** The file's path in its project, for relative links. */
    rel?: string;
    /** Follows a link to another doc (`docs/a.md`, a heading); without it such links do nothing. */
    ondoc?: (doc: string, heading: string | null) => void;
  } = $props();

  const parts = $derived(frontMatter(text));
  // Simple `key: value` lines show as labels; anything else in the front matter stays as text.
  const labels = $derived.by(() => {
    if (!parts.meta) return null;
    const lines = parts.meta.split(/\r?\n/).filter((l) => l.trim());
    const pairs = lines.map((l) => /^([\w.-]+):\s*(.+)$/.exec(l));
    return pairs.every(Boolean) ? pairs.map((m) => ({ key: m![1], value: m![2].replace(/^["']|["']$/g, "") })) : null;
  });
  const html = $derived(renderDoc(parts.body, rel, null));

  function click(e: MouseEvent) {
    const button = (e.target as HTMLElement).closest<HTMLButtonElement>(".cb-copy");
    if (button) {
      copy(button.closest(".codeblock")?.querySelector("code")?.textContent ?? "")
        .then(() => {
          button.textContent = "Copied";
          setTimeout(() => (button.textContent = "Copy"), 1500);
        })
        .catch((err) => hub.fail(err));
      return;
    }
    const a = (e.target as HTMLElement).closest("a");
    if (!a) return;
    e.preventDefault();
    const href = a.getAttribute("href") ?? "";
    if (a.dataset.doc) {
      const { doc, heading } = splitTarget(a.dataset.doc);
      ondoc?.(doc, heading);
    } else if (href.startsWith("#")) {
      a.closest(".reader")?.querySelector(`#h-${CSS.escape(href.slice(1))}`)?.scrollIntoView({ block: "start", behavior: "smooth" });
    } else if (href && !a.dataset.tag) openUrl(href).catch((err) => hub.fail(err));
  }
</script>

<div class="reader">
  {#if labels}
    <div class="props">
      {#each labels as p (p.key)}<span class="prop"><b>{p.key}</b> {p.value}</span>{/each}
    </div>
  {:else if parts.meta}
    <pre class="meta">{parts.meta}</pre>
  {/if}
  <div role="presentation" class="body" onclick={click}>{@html html}</div>
</div>

<style>
  /* Kept in step with the docs tab's reader (DocsView.svelte), a little tighter for the chat. */
  .props { display: flex; flex-wrap: wrap; gap: 6px; margin: 0 0 10px; }
  .prop { background: var(--surface-2); border-radius: var(--r-sm); padding: 1px 8px; font-size: 12px; color: var(--text-3); }
  .prop b { font-weight: 400; color: var(--muted); }
  .meta { margin: 0 0 10px; padding: 6px 10px; border-radius: var(--r-sm); background: var(--surface-2); color: var(--muted); font: 12px var(--mono); white-space: pre-wrap; }

  .body { user-select: text; -webkit-user-select: text; font-size: 14px; line-height: 1.65; color: var(--text-2); overflow-wrap: anywhere; }
  .body :global(p) { margin: 0 0 0.8em; }
  .body :global(h1), .body :global(h2), .body :global(h3), .body :global(h4), .body :global(h5), .body :global(h6) { color: var(--text); font-weight: 500; line-height: 1.3; scroll-margin-top: 8px; }
  .body :global(h1) { font-size: 20px; margin: 1.2em 0 0.5em; }
  .body :global(h2) { font-size: 16px; margin: 1.3em 0 0.4em; }
  .body :global(h3) { font-size: 15px; margin: 1.1em 0 0.3em; }
  .body :global(h4), .body :global(h5), .body :global(h6) { font-size: 14px; margin: 1em 0 0.3em; }
  .body :global(> :first-child) { margin-top: 0; }
  .body :global(> :last-child) { margin-bottom: 0; }
  .body :global(a.wl) { color: var(--accent); text-decoration: underline dotted; text-underline-offset: 3px; }
  .body :global(a.ext) { color: var(--accent); }
  .body :global(a.tag) { text-decoration: none; color: var(--text-3); }
  .body :global(code) { background: var(--surface-2); padding: 1px 4px; border-radius: var(--r-xs); font: 12px var(--mono); }
  .body :global(.codeblock) { background: var(--bg-sidebar); border: 1px solid var(--line); border-radius: var(--r-md); margin: 0 0 1em; overflow: hidden; }
  .body :global(.cb-head) { display: flex; align-items: center; gap: 10px; padding: 4px 8px 4px 12px; border-bottom: 1px solid var(--line); font-size: 12px; color: var(--muted); }
  .body :global(.cb-lang) { color: var(--text-3); }
  .body :global(.cb-copy) { margin-left: auto; color: var(--muted); padding: 1px 8px; border-radius: var(--r-sm); font-size: 12px; }
  .body :global(.cb-copy:hover) { color: var(--text); background: var(--surface-hover); }
  .body :global(.codeblock pre) { margin: 0; padding: 8px 12px; overflow-x: auto; border: 0; max-height: none; }
  .body :global(pre code) { background: none; padding: 0; font-size: 12px; line-height: 1.6; }
  .body :global(ul), .body :global(ol) { padding-left: 1.4em; margin: 0 0 0.8em; }
  .body :global(li > input[type="checkbox"]) { margin: 0 6px 0 -1.2em; vertical-align: -1px; }
  .body :global(li:has(> input[type="checkbox"])) { list-style: none; }
  .body :global(blockquote) { border-left: 2px solid var(--line-strong); margin: 0 0 0.8em; padding: 2px 14px; color: var(--text-3); }
  .body :global(table) { border-collapse: collapse; margin: 0 0 0.8em; font-size: 13px; display: block; max-width: 100%; overflow-x: auto; overflow-wrap: normal; }
  .body :global(td), .body :global(th) { border: 1px solid var(--line); padding: 4px 8px; text-align: left; }
  .body :global(th) { color: var(--text); font-weight: 500; }
  .body :global(hr) { border: 0; border-top: 1px solid var(--line); margin: 1.2em 0; }
  .body :global(img) { max-width: 100%; border-radius: var(--r-md); }
</style>
