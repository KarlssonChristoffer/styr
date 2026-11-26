<!-- Settings page: chat. Used by Settings.svelte. -->
<script lang="ts">
  import { quiet } from "../../quiet";
  import { onMount } from "svelte";
  import { open } from "./core.svelte";
  import { modal } from "../../pop";
  import { api } from "../../api";
  import type { ThreadPrefs } from "../../bindings/ThreadPrefs";
  import { DETAILS, prefs, setPref } from "../../prefs.svelte";
  import { hub } from "../../state.svelte";
  import Switch from "../Switch.svelte";
  import Select from "../Select.svelte";
  import { WORK_SHAPES, WORK_SHAPE_NAMES } from "../../workGrid";

  let threadPrefs = $state<ThreadPrefs | null>(null);

  async function setThreadPrefs(patch: Partial<ThreadPrefs>) {
    if (!threadPrefs) return;
    try {
      threadPrefs = await api.setThreadPrefs({ ...threadPrefs, ...patch });
    } catch (e) {
      hub.fail(e);
    }
  }

  onMount(() => {
    quiet(api.threadPrefs().then((p) => (threadPrefs = p)), "thread prefs");
  });
</script>

<h1>Chat and threads</h1>
<p class="lead">Reading and writing in a thread.</p>
<h2>Message box</h2>
<section>
  <div class="line">
    <div class="what"><div class="name">Messages while it works</div><div class="hint">Queue or steer. Alt Enter does the other.</div></div>
    <div class="control">
      <div class="seg">
        <button class:active={!prefs.steer} onclick={() => setPref("steer", false)}>Queue</button>
        <button class:active={prefs.steer} onclick={() => setPref("steer", true)}>Steer</button>
      </div>
    </div>
  </div>
  <div class="line">
    <div class="what"><div class="name">Files open</div><div class="hint">What a click on a file does.</div></div>
    <div class="control">
      <div class="seg" role="group" aria-label="Files open">
        <button class:active={prefs.fileOpen === "inline"} onclick={() => setPref("fileOpen", "inline")}>In the chat</button>
        <button class:active={prefs.fileOpen === "panel"} onclick={() => setPref("fileOpen", "panel")}>Panel</button>
        <button class:active={prefs.fileOpen === "modal"} onclick={() => setPref("fileOpen", "modal")}>Modal</button>
      </div>
    </div>
  </div>
</section>
<h2>Transcript</h2>
<section>
  <div class="line">
    <div class="what"><div class="name">Chat detail</div></div>
    <div class="control">
      <div class="seg">
        {#each DETAILS as d (d.value)}
          <button class:active={prefs.chatDetail === d.value} data-tip={d.text} onclick={() => setPref("chatDetail", d.value)}>{d.name}</button>
        {/each}
      </div>
    </div>
  </div>
  <div class="line">
    <div class="what"><div class="name">Working indicator</div><div class="hint">Shown while something works.</div></div>
    <div class="control">
      <Select
        label="Working indicator"
        value={prefs.workIndicator}
        options={[...WORK_SHAPES.map((v) => ({ value: v as string, label: WORK_SHAPE_NAMES[v], hint: v === "pour" ? "default" : undefined })), { value: "random", label: "Random" }]}
        onchange={(v) => setPref("workIndicator", v)}
      />
    </div>
  </div>
  <div class="line">
    <div class="what"><div class="name">Math</div><div class="hint">Typeset $$…$$ formulas.</div></div>
    <div class="control"><Switch label="Math" on={prefs.math} onchange={(v) => setPref("math", v)} /></div>
  </div>
</section>
<h2>Thread list</h2>

<section>
  <div class="line">
    <div class="what"><div class="name">Working section</div><div class="hint">Working threads fold below the list.</div></div>
    <div class="control"><Switch label="Fold working threads" on={prefs.workingSection} onchange={(v) => setPref("workingSection", v)} /></div>
  </div>
  <div class="line">
    <div class="what"><div class="name">Thread cards</div></div>
    <div class="control">
      <div class="seg">
        <button class:active={prefs.cards === "comfortable"} onclick={() => setPref("cards", "comfortable")}>Comfortable</button>
        <button class:active={prefs.cards === "compact"} onclick={() => setPref("cards", "compact")}>Compact</button>
        <button class:active={prefs.cards === "dense"} onclick={() => setPref("cards", "dense")}>Dense</button>
      </div>
    </div>
  </div>
  <div class="line">
    <div class="what"><div class="name">Times in the list</div></div>
    <div class="control">
      <div class="seg">
        <button class:active={prefs.listTime === "clock"} onclick={() => setPref("listTime", "clock")}>Clock</button>
        <button class:active={prefs.listTime === "relative"} onclick={() => setPref("listTime", "relative")}>Relative</button>
      </div>
    </div>
  </div>
  <div class="line">
    <div class="what"><div class="name">Group by project</div><div class="hint">A folder per project.</div></div>
    <div class="control"><Switch label="Group threads by project" on={prefs.groupByProject} onchange={(v) => setPref("groupByProject", v)} /></div>
  </div>
  {#if threadPrefs}
    {@const tp = threadPrefs}
    <div class="line">
      <div class="what"><div class="name">Settle idle threads</div><div class="hint">After this long finished. Not pinned.</div></div>
      <div class="control">
        {#each [0, 1, 3, 7] as d (d)}<button class="btn" class:primary={tp.settle_after_days === d} onclick={() => setThreadPrefs({ settle_after_days: d })}>{d ? `${d} ${d === 1 ? "day" : "days"}` : "Off"}</button>{/each}
      </div>
    </div>
    <div class="line">
      <div class="what"><div class="name">Settle when the PR merges</div><div class="hint">When its PR merges. Needs gh.</div></div>
      <div class="control"><Switch label="Settle when the pull request merges" on={tp.settle_on_merge} onchange={(v) => setThreadPrefs({ settle_on_merge: v })} /></div>
    </div>
  {/if}
</section>

<style>
  /* @kit settings */
</style>
