<!-- Where a thread lives, under its title on every page: its project, its machine when there is more than one, its engine when asked for, and how long ago it moved.
     Used by AwayCard.svelte, RecentCard.svelte, WorkersView.svelte, scenes/ThreadRows.svelte. -->
<script lang="ts">
  // Where a thread lives, under its title on every page: its project, its machine when there is
  // more than one, its engine when asked for, and how long ago it moved.
  import type { Thread } from "../bindings/Thread";
  import { ago, folderName, threadProject } from "../format";
  import { engineProvider } from "../providers.svelte";
  import { machineGlyph } from "../refs";
  import { hub } from "../state.svelte";
  import ProviderMark from "./ProviderMark.svelte";
  import { cloudOf } from "../cloud.svelte";
  import { GLYPHS } from "../refs";
  import Ref from "./Ref.svelte";

  let { t, engine = false, when = true, extra }: { t: Thread; engine?: boolean; when?: boolean; extra?: string } = $props();

  const cwd = $derived(threadProject(t));
  const name = $derived((!t.host && hub.projects.find((p) => p.path === cwd)?.name) || folderName(cwd));
  const cloud = $derived(cloudOf(t));
  const host = $derived(!cloud && hub.hosts.length > 1 ? hub.hostInfo(t.host ?? hub.localHost) : undefined);
</script>

<span class="meta">
  <Ref kind="project" project={{ path: cwd, host: t.host }} label={name} title={t.host ? `${t.host}: ${t.cwd}` : t.cwd} />
  {#if cloud}<span class="sep">·</span><span class="cloud" data-tip="Runs on the company's servers, not on a machine of yours"><svg class="kind" width="11" height="11" viewBox="0 0 24 24" aria-hidden="true"><path d={GLYPHS.cloudrun} /></svg>Cloud</span>{/if}
  {#if host}<span class="sep">·</span><Ref kind={machineGlyph(host.name, host.local)} label={host.name} online={host.state === "online"} />{/if}
  {#if engine}<span class="sep">·</span><span class="engine"><ProviderMark id={engineProvider(t.engine).id} size={11} />{t.engine}</span>{/if}
  {#if extra}<span class="sep">·</span><span>{extra}</span>{/if}
  {#if when}<span class="sep">·</span><span>{ago(t.updated_at)}</span>{/if}
</span>

<style>
  .meta { display: inline-flex; align-items: center; gap: 4px; min-width: 0; color: var(--muted); font-size: 12px; white-space: nowrap; }
  .meta :global(.glyph) { width: 11px; height: 11px; margin-right: 4px; }
  .sep { opacity: 0.7; }
  .engine { display: inline-flex; align-items: center; gap: 4px; }
  .cloud { display: inline-flex; align-items: center; gap: 4px; }
  .kind { fill: none; stroke: currentColor; stroke-width: 1.75; stroke-linecap: round; stroke-linejoin: round; }
</style>
