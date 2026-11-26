<!-- What the Inbox is narrowed to, as a divider line like "Pinned": the project (icon, name, count), then the machines as glyphs, then the cross that clears the project. The glyphs adapt: none with one machine, all of them up to four, else the three with most threads and a "+N" that opens the full list.
     Used by Sidebar.svelte. -->
<script lang="ts">
  // What the Inbox is narrowed to, as a divider line like "Pinned": the project (icon, name,
  // count), then the machines as glyphs, then the cross that clears the project. The glyphs adapt:
  // none with one machine, all of them up to four, else the three with most threads and a "+N"
  // that opens the full list.
  import { hub } from "../state.svelte";
  import { threadProject } from "../format";
  import { machineGlyph, GLYPHS } from "../refs";
  import { contextMenu } from "../menu.svelte";
  import { prefs } from "../prefs.svelte";
  import { looks } from "../looks.svelte";
  import Icon from "./Icon.svelte";
  import ProjectIcon from "./ProjectIcon.svelte";

  const scope = $derived(hub.workScope && !prefs.focusOn ? hub.workScope : null);
  const several = $derived(hub.hosts.length > 1);

  /** Threads of the scoped project, on any machine (the glyph counts below are per machine). */
  const inProject = $derived(
    scope ? hub.active.filter((t) => (scope.quick ? true : threadProject(t) === scope.cwd && (t.host ?? null) === scope.host)) : hub.active,
  );
  const count = (host: string) => inProject.filter((t) => hub.hostOf(t) === host).length;

  const ranked = $derived(
    hub.hosts
      .map((h, i) => ({ h, n: count(h.name), i }))
      .sort((a, b) => b.n - a.n || a.i - b.i),
  );
  const fits = $derived(ranked.length <= 4);
  const shown = $derived.by(() => {
    if (fits) return hub.hosts.map((h) => ({ h, n: count(h.name) }));
    const top = ranked.slice(0, 3);
    const pick = ranked.find((r) => r.h.name === hub.hostFilter);
    if (pick && !top.includes(pick)) top[2] = pick;
    return top;
  });
  const more = $derived(fits ? 0 : ranked.length - shown.length);

  function toggle(name: string) {
    hub.hostFilter = hub.hostFilter === name ? null : name;
  }

  function openAll(e: MouseEvent) {
    const r = (e.currentTarget as HTMLElement).getBoundingClientRect();
    contextMenu.show(r.left, r.bottom + 4, [
      { label: "All machines", hint: hub.hostFilter ? "" : "✓", run: () => (hub.hostFilter = null) },
      null,
      ...ranked.map(({ h, n }) => ({
        label: h.name,
        hint: hub.hostFilter === h.name ? "✓" : h.state !== "online" ? h.state : n ? String(n) : "",
        run: () => (hub.hostFilter = h.name),
      })),
    ]);
  }
</script>

{#if scope || several}
  <div class="scope">
    {#if scope}
      <span class="who" data-tip={scope.cwd}>
        <ProjectIcon path={scope.cwd} host={scope.host} name={scope.name} size={13} />
        <b class="ellipsis" style:color="var(--p-{looks.project(scope.cwd, scope.host, scope.name).color})">{scope.name}</b>
        <span class="n">{inProject.filter((t) => !hub.hostFilter || hub.hostOf(t) === hub.hostFilter).length}</span>
      </span>
    {:else}
      <span class="who all">All projects</span>
    {/if}
    <i class="rule"></i>
    {#if several}
      <span class="seg" role="group" aria-label="Machine">
        {#each shown as { h, n } (h.name)}
          <button
            class="m"
            class:on={hub.hostFilter === h.name}
            class:offline={h.state !== "online"}
            aria-pressed={hub.hostFilter === h.name}
            data-tip="{h.name}{h.state === 'online' ? '' : ` is ${h.state}`}{n ? ` · ${n}` : ''}"
            onclick={() => toggle(h.name)}
          >
            <svg viewBox="0 0 24 24" aria-hidden="true"><path d={GLYPHS[machineGlyph(h.name, !!h.local)]} /></svg>
            {#if fits && n}<em>{n}</em>{/if}
          </button>
        {/each}
        {#if more}<button class="m more" data-tip="All {ranked.length} machines" onclick={openAll}>+{more}</button>{/if}
      </span>
    {/if}
    {#if scope}
      <button class="x" data-tip="Show every project's threads" aria-label="Show every project" onclick={() => (hub.workScope = null)}><Icon name="x" size={11} /></button>
    {/if}
  </div>
{/if}

<style>
  .scope { display: flex; align-items: center; gap: 8px; padding: 0 4px 6px 6px; min-height: 30px; font-size: 12px; color: var(--muted); }
  .who { display: inline-flex; align-items: center; gap: 8px; min-width: 0; }
  .who b { font-weight: 600; }
  .who.all { color: var(--muted); }
  .who :global(svg) { flex: none; }
  .n { font-size: 11px; color: var(--muted); }
  .rule { flex: 1; height: 1px; background: var(--line); min-width: 6px; }
  .ellipsis { white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
  .seg { display: inline-flex; gap: 2px; padding: 2px; border-radius: var(--r-md); background: var(--surface); flex: none; }
  .m { display: inline-flex; align-items: center; gap: 3px; padding: 3px 6px; border-radius: var(--r-sm); color: var(--muted); font-size: 11px; }
  .m svg { width: 13px; height: 13px; fill: none; stroke: currentColor; stroke-width: 1.75; stroke-linecap: round; stroke-linejoin: round; }
  .m em { font-style: normal; }
  .m:hover { color: var(--text-2); }
  .m.on { background: var(--surface-hover); color: var(--text); }
  .m.offline { opacity: 0.6; }
  .x { display: grid; place-items: center; width: 20px; height: 20px; border-radius: var(--r-sm); color: var(--muted); flex: none; }
  .x:hover { color: var(--text); background: var(--surface); }
</style>
