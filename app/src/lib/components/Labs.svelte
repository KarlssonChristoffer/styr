<!-- Labs: experiments live here until they have earned a place elsewhere. Each one is a self-contained component, so moving it to Home or the project page is one line. The activity map graduated to Overview.
     Used by App.svelte. -->
<script lang="ts">
  // Labs: experiments live here until they have earned a place elsewhere. Each one is a
  // self-contained component, so moving it to Home or the project page is one line.
  // The activity map graduated to Overview.
  import BenchCard from "./BenchCard.svelte";
  import DevMode from "./DevMode.svelte";
  import SimInbox from "./SimInbox.svelte";
</script>

<div class="labs">
  <div class="page-fill">
    <h1>Labs</h1>
    <p class="lead">Experiments to try before they get a home.</p>

    <!-- Side by side when there is room. -->
    <div class="tiles" style:--tile="520px">
      <div>
        <h2>Work on Hub</h2>
        <DevMode />
      </div>
      <div>
        <h2>Simulate the inbox</h2>
        <SimInbox />
      </div>
      <div>
        <h2>Benchmark Hub</h2>
        <BenchCard />
      </div>
    </div>
  </div>
</div>

<style>
  .labs { flex: 1; overflow-y: auto; min-width: 0; }
  h1 { color: var(--text); font-size: var(--title); font-weight: 500; letter-spacing: -0.01em; margin: 0 0 14px; }
  h2 { color: var(--muted); font-size: 12px; font-weight: 500; margin: 8px 0 6px; }
  .lead { color: var(--text-3); margin: -6px 0 14px; }
</style>
