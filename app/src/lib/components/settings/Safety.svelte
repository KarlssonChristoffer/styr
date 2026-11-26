<!-- Settings page: safety. Used by Settings.svelte. -->
<script lang="ts">
  import { quiet } from "../../quiet";
  import { onMount } from "svelte";
  import { sc, edit, cap, guard } from "./core.svelte";
  import Quiet from "../Quiet.svelte";
  import { api } from "../../api";
  import type { PermissionLevel } from "../../bindings/PermissionLevel";
  import type { PermissionPrefs } from "../../bindings/PermissionPrefs";
  import { hub } from "../../state.svelte";
  import Switch from "../Switch.svelte";
  import Customs from "../Customs.svelte";
  import RuleList from "../RuleList.svelte";

  const saved = $derived(sc.core?.saved);

  // What threads may do without asking: Hub answers the engine's permission questions it covers.
  let permits = $state<PermissionPrefs | null>(null);

  async function permit(patch: Partial<PermissionPrefs>) {
    if (!permits) return;
    try {
      permits = await api.setPermissions({ ...permits, ...patch });
    } catch (e) {
      hub.fail(e);
    }
  }

  const LEVELS: [PermissionLevel, string, string][] = [
    ["ask", "Ask me", "Every question the engine asks comes to you."],
    ["read", "Read", "Reading, searching and fetching go ahead. Edits and commands ask."],
    ["edit", "Edit files", "Reading and editing files go ahead. Commands and deletes ask."],
    ["everything", "Everything", "All goes ahead, except deleting files, commands that destroy work (rm, git reset --hard, force push) and leaving plan mode."],
  ];

  const families = ["opus", "sonnet", "haiku"];

  onMount(() => {
    quiet(api.permissions().then((p) => (permits = p)), "permissions");
  });
</script>

<h1>Safety</h1>
<p class="lead">Rules every thread follows.</p>
{#if permits}
  {@const p = permits}
  <section>
    <div class="line">
      <div class="what">
        <div class="name">Without asking, threads may</div>
        <div class="hint">{LEVELS.find(([l]) => l === p.level)?.[2]}</div>
      </div>
      <div class="control">
        <div class="seg">
          {#each LEVELS as [level, label] (level)}
            <button class:active={p.level === level} onclick={() => permit({ level })}>{label}</button>
          {/each}
        </div>
      </div>
    </div>
    <div class="line col">
      <div class="what">
        <div class="name">Always allow</div>
        <div class="hint">Never asks. <span class="mono">*</span> matches anything.</div>
        <RuleList items={p.allow} placeholder="npm test*, mcp__github__*" onchange={(allow) => permit({ allow })} />
      </div>
    </div>
    <div class="line col">
      <div class="what">
        <div class="name">Always refuse</div>
        <div class="hint">Always refused.</div>
        <RuleList items={p.deny} placeholder="git push*" onchange={(deny) => permit({ deny })} />
      </div>
    </div>
    <div class="line col">
      <div class="what">
        <div class="name">Protected files</div>
        <div class="hint">Agents cannot edit these.</div>
        <RuleList items={p.protected} placeholder=".env*, scripts/release/" onchange={(list) => permit({ protected: list })} />
      </div>
    </div>
  </section>
{/if}
{#if hub.guard && saved}
  {@const g = hub.guard}
  <section>
    <div class="line">
      <div class="what"><div class="name">Undo snapshots</div></div>
      <div class="control"><Switch label="Undo snapshots" on={saved.checkpoints} onchange={(v) => edit("checkpoints", v)} /></div>
    </div>
    <div class="line">
      <div class="what"><div class="name">Second opinions by</div><div class="hint">Auto picks another model.</div></div>
      <div class="control">
        <button class="btn" class:primary={!g.review_model} onclick={() => guard({ review_model: null })}>Auto</button>
        {#each families as m (m)}<button class="btn" class:primary={g.review_model === m} onclick={() => guard({ review_model: m })}>{cap(m)}</button>{/each}
      </div>
    </div>
    <div class="line">
      <div class="what"><div class="name">Proof before done</div><div class="hint">Run tests or take a screenshot before done.</div></div>
      <div class="control"><Switch label="Proof before done" on={g.proof} onchange={(v) => guard({ proof: v })} /></div>
    </div>
    <div class="line">
      <div class="what"><div class="name">Docs in step</div><div class="hint">Check a doc when code it covers changes.</div></div>
      <div class="control"><Switch label="Docs in step" on={g.docs_check} onchange={(v) => guard({ docs_check: v })} /></div>
    </div>
    <div class="line">
      <div class="what"><div class="name">Leak guard</div><div class="hint">Move keys and tokens out of what an agent writes.</div></div>
      <div class="control"><Switch label="Leak guard" on={g.leak_guard} onchange={(v) => guard({ leak_guard: v })} /></div>
    </div>
    <div class="line">
      <div class="what"><div class="name">Customs check</div><div class="hint">Check new dependencies and CDN libraries.</div></div>
      <div class="control"><Switch label="Customs check" on={g.customs} onchange={(v) => guard({ customs: v })} /></div>
    </div>
    <div class="line">
      <div class="what"><div class="name">Clash warnings</div><div class="hint">Warn when two threads change the same file.</div></div>
      <div class="control"><Switch label="Clash warnings" on={g.clash_warn} onchange={(v) => guard({ clash_warn: v })} /></div>
    </div>
    <div class="line">
      <div class="what"><div class="name">Stay on the stage</div><div class="hint">Say when a new thread is not part of the stage.</div></div>
      <div class="control"><Switch label="Stay on the stage" on={g.focus_check} onchange={(v) => guard({ focus_check: v })} /></div>
    </div>
  </section>
{:else}
  <section><Quiet kind="loading" /></section>
{/if}

<style>
  /* @kit settings */
</style>
