<!-- One thing Hub knows about, the same way on every page: a grey glyph for what it is, then its name. Colour never says the kind, only state (a thread's ring, a machine's online dot, a pull request's glyph). Pass `onopen` to make it a link; inside a row that is already a button, leave it out.
     Used by AwayCard.svelte, CommandPalette.svelte, Customs.svelte, DayCard.svelte and 16 more. -->
<script lang="ts">
  // One thing Hub knows about, the same way on every page: a grey glyph for what it is, then its
  // name. Colour never says the kind, only state (a thread's ring, a machine's online dot, a pull
  // request's glyph). Pass `onopen` to make it a link; inside a row that is already a button,
  // leave it out.
  import { GLYPHS, type Glyph } from "../refs";
  import ProjectIcon from "./ProjectIcon.svelte";

  let {
    kind,
    label,
    title,
    mono = false,
    online,
    tone,
    onopen,
    project,
  }: {
    kind: Glyph;
    label: string;
    title?: string;
    mono?: boolean;
    /** Machines and services: a green dot while it is up. */
    online?: boolean;
    /** Pull requests and issues: open, draft, merged or closed. */
    tone?: string;
    onopen?: () => void;
    /** A project's folder (and machine, if another one): shows its own coloured icon instead. */
    project?: { path: string; host?: string | null };
  } = $props();

  function key(e: KeyboardEvent) {
    if (e.key === "Enter") onopen?.();
  }
</script>

{#snippet mark()}
  {#if project}<span class="pi"><ProjectIcon path={project.path} host={project.host} name={label} size={13} /></span>{:else}<svg class="glyph {tone ?? ''}" viewBox="0 0 24 24" aria-hidden="true"><path d={GLYPHS[kind]} /></svg>{/if}
{/snippet}

{#if onopen}
  <span class="ref link" class:mono role="link" tabindex="0" data-tip={title} onclick={(e) => { e.stopPropagation(); onopen(); }} onkeydown={key}>
    {@render mark()}<span class="ref-label">{label}</span>{#if online}<i class="up"></i>{/if}
  </span>
{:else}
  <span class="ref" class:mono data-tip={title}>
    {@render mark()}<span class="ref-label">{label}</span>{#if online}<i class="up"></i>{/if}
  </span>
{/if}

<style>
  .ref { display: inline-flex; align-items: center; min-width: 0; white-space: nowrap; color: inherit; }
  .pi { display: inline-flex; margin-right: 4px; }
  .ref-label { overflow: hidden; text-overflow: ellipsis; }
  .mono .ref-label { font-family: var(--mono); font-size: 0.92em; }
  .link { cursor: pointer; }
  .link:hover .ref-label { text-decoration: underline; text-underline-offset: 3px; text-decoration-color: var(--line-strong); }
</style>
