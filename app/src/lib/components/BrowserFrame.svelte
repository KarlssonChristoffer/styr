<!-- A dev server the thread runs, big in the right panel. Unlike an agent's page, this is the user's own app, so it keeps its own origin (localhost:<port>, never Hub's: `preview.ts`) with storage, forms and logins, and hot reload works as in any browser.
     Used by Viewer.svelte. -->
<script lang="ts">
  // A dev server the thread runs, big in the right panel. Unlike an agent's page, this is the
  // user's own app, so it keeps its own origin (localhost:<port>, never Hub's: `preview.ts`)
  // with storage, forms and logins, and hot reload works as in any browser.
  let { url }: { url: string } = $props();

  let generation = $state(0);
  /** Loads the address again, from the start. */
  export function reload() {
    generation++;
  }
</script>

{#key `${generation}:${url}`}
  <iframe
    src={url}
    title={url}
    sandbox="allow-scripts allow-same-origin allow-forms allow-popups allow-modals allow-downloads"
    referrerpolicy="no-referrer"
  ></iframe>
{/key}

<style>
  iframe { flex: 1; min-height: 0; width: 100%; border: 0; display: block; background: #fff; }
</style>
