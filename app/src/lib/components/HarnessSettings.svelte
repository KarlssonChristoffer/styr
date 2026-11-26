<!-- A harness's own settings (Hermes: its tools, memory, compression, steps, thinking), read and changed through its own program on this machine (`harness_settings.rs`). A change here reaches every machine with the engines.
     Used by EngineList.svelte. -->
<script lang="ts">
  // A harness's own settings (Hermes: its tools, memory, compression, steps, thinking), read and
  // changed through its own program on this machine (`harness_settings.rs`). A change here
  // reaches every machine with the engines.
  import { api } from "../api";
  import type { HarnessSettings } from "../bindings/HarnessSettings";
  import type { HarnessSetting } from "../bindings/HarnessSetting";
  import { hub } from "../state.svelte";
  import Select from "./Select.svelte";
  import Switch from "./Switch.svelte";

  let { harness, host = null }: { harness: string; host?: string | null } = $props();

  let page = $state<HarnessSettings | null>(null);
  let failed = $state("");
  /** The key being written, so its control waits. */
  let saving = $state("");

  $effect(() => {
    const h = harness;
    page = null;
    failed = "";
    api
      .harnessSettings(h, host)
      .then((p) => (page = p))
      .catch((e) => (failed = e instanceof Error ? e.message : String(e)));
  });

  async function set(s: HarnessSetting, value: unknown) {
    saving = s.key;
    failed = "";
    try {
      page = await api.setHarnessSetting(harness, s.key, value, host);
    } catch (e) {
      failed = e instanceof Error ? e.message : String(e);
    } finally {
      saving = "";
    }
  }

  const key = (v: unknown) => JSON.stringify(v ?? null);
  const off = (s: HarnessSetting): string[] => (Array.isArray(s.value) ? (s.value as string[]) : []);
  function flip(s: HarnessSetting, name: string) {
    const now = off(s);
    set(s, now.includes(name) ? now.filter((n) => n !== name) : [...now, name]);
  }
</script>

<div class="harness-settings">
  {#if !page && !failed}
    <div class="hint">Asking {harness} for its settings…</div>
  {:else if page}
    <div class="row">
      <div class="what">
        <div class="name">Version</div>
        <div class="hint">
          {#if page.version}{page.version} here{:else if page.problem}{page.problem}{:else}Unknown{/if}
          · changes here reach every machine
        </div>
      </div>
      <button class="btn" onclick={() => hub.openSettings("updates")}>Updates</button>
    </div>
    {#if !page.problem}
      {#each page.settings as s (s.key)}
        <div class="row" class:top={s.kind === "off_list"}>
          <div class="what">
            <div class="name">{s.label}</div>
            <div class="hint">{s.what}</div>
            {#if s.kind === "off_list"}
              <div class="chips">
                {#each s.choices as c (key(c.value))}
                  {@const isOff = off(s).includes(String(c.value))}
                  <button class="chip" class:off={isOff} disabled={saving === s.key} data-tip={c.what} aria-pressed={!isOff} onclick={() => flip(s, String(c.value))}>{c.name}</button>
                {/each}
              </div>
            {/if}
          </div>
          {#if s.kind === "toggle"}
            <Switch on={s.value === true} label={s.label} disabled={saving === s.key} onchange={(on) => set(s, on)} />
          {:else if s.kind === "choice"}
            <Select
              value={key(s.value)}
              size="small"
              label={s.label}
              disabled={saving === s.key}
              options={s.choices.map((c) => ({ value: key(c.value), label: c.name, hint: c.what || undefined }))}
              onchange={(v) => set(s, JSON.parse(v))}
            />
          {/if}
        </div>
      {/each}
    {/if}
  {/if}
  {#if failed}<div class="hint bad">{failed}</div>{/if}
</div>

<style>
  .harness-settings { display: flex; flex-direction: column; margin: 4px 0 6px; padding: 2px 14px; border: 1px solid var(--line); border-radius: var(--r-md); background: var(--surface); }
  .row { display: flex; align-items: center; gap: 24px; padding: 10px 0; }
  .row + .row { border-top: 1px solid var(--line); }
  .row.top { align-items: flex-start; }
  .what { flex: 1; min-width: 0; display: flex; flex-direction: column; gap: 3px; }
  .name { color: var(--text); font-size: 13px; }
  .hint { color: var(--muted); font-size: 13px; }
  .hint.bad { color: var(--red); padding: 8px 0; }
  .chips { display: flex; flex-wrap: wrap; gap: 6px; margin-top: 6px; }
  .chip { height: 24px; padding: 0 8px; border-radius: var(--r-md); border: 1px solid var(--line-strong); color: var(--text-2); font-size: 12px; }
  .chip:hover { color: var(--text); background: var(--surface-hover); }
  .chip.off { color: var(--muted); border-style: dashed; text-decoration: line-through; }
  .chip:disabled { opacity: 0.6; }
</style>
