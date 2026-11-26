<!-- Settings page: privacy. Used by Settings.svelte. -->
<script lang="ts">
  import { sc, openFolder } from "./core.svelte";

  const about = $derived(sc.about);


</script>

<h1>Privacy and data</h1>
<p class="lead">Hub keeps everything on this computer and sends nothing anywhere itself.</p>
<section>
  <div class="line">
    <div class="what"><div class="name">Where Hub keeps its data</div></div>
    <div class="control"><button class="btn" disabled={!about} onclick={() => about && openFolder(about.data_dir)}>Open folder</button></div>
  </div>
</section>

<style>
  /* @kit settings */
</style>
