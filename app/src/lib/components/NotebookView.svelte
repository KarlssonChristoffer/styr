<!-- A Jupyter notebook read the way Jupyter shows it: Markdown cells rendered, code cells coloured with their run number, and what they printed below them: text, tables as text, pictures and errors. HTML output is left as its text, so nothing in a notebook runs inside Hub.
     Used by FilePreview.svelte. -->
<script lang="ts">
  // A Jupyter notebook read the way Jupyter shows it: Markdown cells rendered, code cells coloured
  // with their run number, and what they printed below them: text, tables as text, pictures and
  // errors. HTML output is left as its text, so nothing in a notebook runs inside Hub.
  import { highlight } from "../code";
  import { parse as ansi } from "../ansi";
  import DocReader from "./DocReader.svelte";

  let { text }: { text: string } = $props();

  type Out = { kind: "text" | "error"; text: string } | { kind: "image"; src: string } | { kind: "md"; text: string };
  type Cell = { type: "markdown" | "code" | "raw"; source: string; count: number | null; outputs: Out[] };

  const join = (s: unknown) => (Array.isArray(s) ? s.join("") : typeof s === "string" ? s : "");
  const book = $derived.by(() => {
    try {
      const nb = JSON.parse(text);
      const lang: string = nb.metadata?.language_info?.name ?? nb.metadata?.kernelspec?.language ?? "python";
      const cells: Cell[] = (nb.cells ?? []).map((c: Record<string, unknown>) => ({
        type: c.cell_type === "markdown" ? "markdown" : c.cell_type === "code" ? "code" : "raw",
        source: join(c.source),
        count: typeof c.execution_count === "number" ? c.execution_count : null,
        outputs: ((c.outputs as Record<string, unknown>[] | undefined) ?? []).flatMap((o): Out[] => {
          if (o.output_type === "stream") return [{ kind: o.name === "stderr" ? "error" : "text", text: join(o.text) }];
          if (o.output_type === "error") return [{ kind: "error", text: join(o.traceback) || `${o.ename}: ${o.evalue}` }];
          const data = (o.data ?? {}) as Record<string, unknown>;
          for (const mime of ["image/png", "image/jpeg", "image/gif"]) {
            if (data[mime]) return [{ kind: "image", src: `data:${mime};base64,${join(data[mime]).replace(/\s/g, "")}` }];
          }
          if (data["image/svg+xml"]) return [{ kind: "image", src: `data:image/svg+xml;charset=utf-8,${encodeURIComponent(join(data["image/svg+xml"]))}` }];
          if (data["text/markdown"]) return [{ kind: "md", text: join(data["text/markdown"]) }];
          if (data["text/plain"]) return [{ kind: "text", text: join(data["text/plain"]) }];
          return [];
        }),
      }));
      return { lang, cells, error: null };
    } catch (e) {
      return { lang: "", cells: [] as Cell[], error: e instanceof Error ? e.message : String(e) };
    }
  });
</script>

{#if book.error}
  <p class="bad">This notebook does not read as JSON: {book.error}</p>
{:else}
  <div class="nb">
    {#each book.cells as cell, i (i)}
      {#if cell.type === "markdown"}
        <div class="md-cell"><DocReader text={cell.source} /></div>
      {:else if cell.type === "code"}
        <div class="code-cell">
          <span class="count">[{cell.count ?? " "}]</span>
          <pre class="src"><code class="hljs">{@html highlight(cell.source, book.lang).html}</code></pre>
        </div>
        {#each cell.outputs as out, j (j)}
          <div class="out">
            {#if out.kind === "image"}
              <img src={out.src} alt="Output of cell {cell.count ?? i + 1}" />
            {:else if out.kind === "md"}
              <DocReader text={out.text} />
            {:else}
              <pre class:err={out.kind === "error"}>{#each ansi(out.text) as line, k (k)}{#each line as s, m (m)}{#if s.cls}<span class={s.cls}>{s.text}</span>{:else}{s.text}{/if}{/each}{"\n"}{/each}</pre>
            {/if}
          </div>
        {/each}
      {:else}
        <pre class="raw">{cell.source}</pre>
      {/if}
    {/each}
  </div>
{/if}

<style>
  .nb { display: flex; flex-direction: column; gap: 8px; }
  .md-cell { padding: 2px 0 2px 44px; }
  .code-cell { display: flex; gap: 8px; align-items: flex-start; }
  .count { width: 36px; flex: none; text-align: right; color: var(--muted); font: 12px/1.6 var(--mono); padding-top: 8px; }
  .src { flex: 1; min-width: 0; margin: 0; padding: 8px 12px; background: var(--bg-sidebar); border: 1px solid var(--line); border-radius: var(--r-md); overflow-x: auto; font: 12px/1.6 var(--mono); }
  .out { padding-left: 44px; }
  .out pre { margin: 0; padding: 2px 0; white-space: pre-wrap; overflow-wrap: anywhere; color: var(--text-3); font: 12px/1.55 var(--mono); max-height: 360px; overflow: auto; }
  .out pre.err { color: var(--red); }
  .out img { max-width: 100%; border-radius: var(--r-sm); background: #fff; }
  .raw { margin: 0 0 0 44px; color: var(--muted); font: 12px var(--mono); white-space: pre-wrap; }
  .bad { color: var(--muted); }
  .nb :global(.c-red) { color: var(--red); }
  .nb :global(.c-green) { color: var(--green); }
  .nb :global(.c-amber) { color: var(--amber); }
  .nb :global(.c-blue), .nb :global(.c-cyan), .nb :global(.c-magenta) { color: var(--accent); }
  .nb :global(.c-dim) { color: var(--muted); }
</style>
