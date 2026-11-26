<!-- The frame of a view that is a page: one scroll area, the gutter, one title and one width. Widths: reading (760) for text and forms; list (rows) and wide (grids) fill the window; `full` takes the room it is given, for a page beside a panel. `head` is the rest of the title row (a count, a button); the title itself is always the same. `fill` is for a page whose body scrolls on its own (a document), so the frame does not.
     Used by LibraryView.svelte, MachinesView.svelte, PluginsView.svelte, RecipesView.svelte and 2 more. -->
<script lang="ts">
  // The frame of a view that is a page: one scroll area, the gutter, one title and one width.
  // Widths: reading (760) for text and forms; list (rows) and wide (grids) fill the window;
  // `full` takes the room it is given, for a page beside a panel.
  // `head` is the rest of the title row (a count, a button); the title itself is always the same.
  // `fill` is for a page whose body scrolls on its own (a document), so the frame does not.
  import type { Snippet } from "svelte";

  let {
    title,
    width = "list",
    fill = false,
    head,
    children,
  }: {
    title: string;
    width?: "reading" | "list" | "wide" | "full";
    fill?: boolean;
    head?: Snippet;
    children: Snippet;
  } = $props();
</script>

<div class="page" class:fill>
  <div class="frame {width}">
    <header>
      <h1>{title}</h1>
      {@render head?.()}
    </header>
    {@render children()}
  </div>
</div>

<style>
  .page { flex: 1; min-height: 0; overflow-y: auto; padding: var(--page-top) var(--gutter) 60px; width: 100%; min-width: 0; container-type: inline-size; }
  .page.fill { overflow: hidden; display: flex; flex-direction: column; padding-bottom: 20px; }
  .frame { font-size: 13px; }
  .page.fill .frame { flex: 1; min-height: 0; display: flex; flex-direction: column; max-width: none; }
  .reading { max-width: 760px; }
  /* list and wide fill the window for now; one place to cap them later. */
  .list, .wide { max-width: none; }
  .full { max-width: none; }
  header { display: flex; align-items: center; gap: 12px; min-height: 28px; }
  h1 { color: var(--text); font-size: 20px; font-weight: 600; letter-spacing: -0.01em; line-height: 28px; margin: 0; }
</style>
