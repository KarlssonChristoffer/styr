<!-- Rows of cells as a table to look through: the header stays on top, a column sorts on click (numbers as numbers), a filter finds rows, numbers line up on the right, and very many rows show a thousand at a time.
     Used by FilePreview.svelte. -->
<script lang="ts">
  // Rows of cells as a table to look through: the header stays on top, a column sorts on click
  // (numbers as numbers), a filter finds rows, numbers line up on the right, and very many rows
  // show a thousand at a time.
  let { columns, rows }: { columns: string[]; rows: string[][] } = $props();

  const PAGE = 1000;
  let sortBy = $state<number | null>(null);
  let down = $state(false);
  let filter = $state("");
  let limit = $state(PAGE);

  const isNum = (s: string) => s.trim() !== "" && !isNaN(Number(s.replace(/\s/g, "").replace(/,(?=\d+$)/, ".")));
  const toNum = (s: string) => Number(s.replace(/\s/g, "").replace(/,(?=\d+$)/, "."));
  // A column is numeric when nearly all its filled cells are numbers.
  const numeric = $derived(
    columns.map((_, c) => {
      const filled = rows.slice(0, 500).map((r) => r[c] ?? "").filter((s) => s.trim() !== "");
      return filled.length > 0 && filled.filter(isNum).length >= filled.length * 0.9;
    }),
  );
  const filtered = $derived.by(() => {
    const q = filter.trim().toLowerCase();
    const list = rows.map((r, i) => ({ r, i })).filter(({ r }) => !q || r.some((c) => c.toLowerCase().includes(q)));
    if (sortBy === null) return list;
    const c = sortBy;
    const num = numeric[c];
    list.sort((a, b) => {
      const x = a.r[c] ?? "", y = b.r[c] ?? "";
      const d = num ? toNum(x || "NaN") - toNum(y || "NaN") || 0 : x.localeCompare(y, undefined, { numeric: true, sensitivity: "base" });
      return down ? -d : d;
    });
    return list;
  });
  function sort(c: number) {
    if (sortBy === c) down ? (sortBy = null) : (down = true);
    else (sortBy = c), (down = false);
  }
</script>

<div class="table">
  <div class="bar">
    <span>{rows.length} {rows.length === 1 ? "row" : "rows"} · {columns.length} {columns.length === 1 ? "column" : "columns"}</span>
    {#if filter}<span>· {filtered.length} match</span>{/if}
    <input class="filter" placeholder="Filter rows" bind:value={filter} oninput={() => (limit = PAGE)} />
  </div>
  <div class="scroll">
    <table>
      <thead>
        <tr>
          <th class="n">#</th>
          {#each columns as col, c (c)}
            <th class:num={numeric[c]}><button onclick={() => sort(c)}>{col || " "}{#if sortBy === c}<span class="dir">{down ? "↓" : "↑"}</span>{/if}</button></th>
          {/each}
        </tr>
      </thead>
      <tbody>
        {#each filtered.slice(0, limit) as { r, i } (i)}
          <tr>
            <td class="n">{i + 1}</td>
            {#each columns as _, c (c)}<td class:num={numeric[c]} title={(r[c] ?? "").length > 60 ? r[c] : undefined}>{r[c] ?? ""}</td>{/each}
          </tr>
        {/each}
      </tbody>
    </table>
    {#if filtered.length > limit}
      <button class="more" onclick={() => (limit += PAGE)}>Show {Math.min(PAGE, filtered.length - limit)} more of {filtered.length - limit}</button>
    {/if}
  </div>
</div>

<style>
  .table { display: flex; flex-direction: column; min-height: 0; }
  .bar { display: flex; align-items: center; gap: 6px; padding: 0 0 6px; color: var(--muted); font-size: 12px; }
  .filter { margin-left: auto; width: 160px; height: 22px; padding: 0 8px; border-radius: var(--r-sm); border: 1px solid var(--line); background: var(--surface); color: var(--text-2); font-size: 12px; }
  .filter:focus { outline: none; border-color: var(--line-strong); }
  .scroll { overflow: auto; max-height: inherit; border: 1px solid var(--line); border-radius: var(--r-md); min-height: 0; flex: 1; }
  table { border-collapse: collapse; font-size: 13px; width: max-content; min-width: 100%; }
  th, td { padding: 3px 10px; border-bottom: 1px solid var(--line); text-align: left; white-space: nowrap; max-width: 320px; overflow: hidden; text-overflow: ellipsis; }
  th { position: sticky; top: 0; z-index: 1; background: var(--surface-2); color: var(--text); font-weight: 500; }
  th button { font: inherit; color: inherit; padding: 0; }
  .dir { margin-left: 4px; color: var(--muted); }
  td { color: var(--text-2); }
  .num { text-align: right; font-variant-numeric: tabular-nums; }
  .n { color: var(--muted); text-align: right; font-variant-numeric: tabular-nums; position: sticky; left: 0; background: var(--bg-sidebar); }
  th.n { z-index: 2; background: var(--surface-2); }
  tbody tr:hover td { background: var(--surface-hover); }
  .more { display: block; margin: 6px auto; color: var(--muted); font-size: 12px; }
  .more:hover { color: var(--text); }
</style>
