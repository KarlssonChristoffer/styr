<!-- The use of one API connection that Hub itself makes: a question to try, which of Hub's decisions
     ask it, the tune's suggestions and the log of what it decided. Used by ApiConnections.svelte. -->
<script lang="ts">
  import { api } from "../api";
  import { apis } from "../apis.svelte";
  import type { ApiAnswer } from "../bindings/ApiAnswer";
  import type { ApiConnection } from "../bindings/ApiConnection";
  import type { ApiDecision } from "../bindings/ApiDecision";
  import type { ApiPrefs } from "../bindings/ApiPrefs";
  import type { TuneSuggestion } from "../bindings/TuneSuggestion";
  import { ago, clock } from "../format";
  import { hub } from "../state.svelte";
  import Switch from "./Switch.svelte";
  import { FEATURE_NAMES, KINDS, SAID, pct } from "../apiConnections";

  let { c, s, decisions, save, reload }: {
    c: ApiConnection;
    s: ReturnType<typeof apis.status>;
    decisions: ApiDecision[];
    save: (change: (p: ApiPrefs) => void) => Promise<void>;
    reload: () => Promise<unknown>;
  } = $props();

  const data = $derived(apis.data);
  /** How much of the decision log shows; the status bar card links here for the whole of it. */
  let logShown = $state(100);
  let tryText = $state("The build has failed on every push since this morning and nobody can merge anything.");
  let tryStatement = $state("The message describes something urgent");
  let tryAnswer = $state<ApiAnswer | null>(null);
  let asking = $state(false);
  const mine = $derived(decisions.filter((d) => d.connection === c.name));
  // A new page for each connection: what was tried for the last one doesn't carry over.
  $effect(() => {
    void c.name;
    tryAnswer = null;
  });

  async function ask() {
    asking = true;
    try {
      tryAnswer = await api.tryApi(c.name, { state: tryText, statement: tryStatement });
    } catch (e) {
      hub.fail(e);
    } finally {
      asking = false;
      await reload();
    }
  }

  /** A feature is on unless turned off, for the connection Hub's decisions ask. */
  const featureOn = (f: string) => !!data && !data.prefs.off.includes(f) && (data.prefs.helper ?? c.name) === c.name;
  function setFeature(f: string, on: boolean) {
    save((p) => {
      p.off = on ? p.off.filter((x) => x !== f) : [...p.off, f];
      if (on && p.helper && p.helper !== c.name) p.helper = c.name;
    });
  }

  /** Opens a thread in Hub's own project that makes one suggestion, and marks it used. */
  async function build(label: string, sg: TuneSuggestion) {
    const own = hub.projects.find((p) => /[\\/]hub[\\/]?$/i.test(p.path))?.path;
    const when = sg.backing.map((at) => new Date(at).toISOString().slice(0, 16).replace("T", " ") + " UTC").join(", ");
    const ask =
      `Make this change to ${label}, Hub's decision model, for the "${sg.feature}" feature (${KINDS[sg.kind] ?? sg.kind}):\n\n${sg.text}\n\n` +
      `It comes from a tune of ${label}'s decisions${when ? `; the decisions that back it were made at ${when}` : ""}. ` +
      `What each decision read is kept in apis.seen (see jev_tune.rs). Keep the change to what the suggestion asks.`;
    try {
      await apis.answer(sg.id, true);
      hub.startDraft(own ?? hub.draftFolder(), ask);
    } catch (e) {
      hub.fail(e);
    }
  }
</script>

  <h2>Try a question</h2>
  <section>
    <div class="line top">
      <div class="what">
        <textarea class="field area" rows="2" bind:value={tryText} spellcheck="false"></textarea>
        <input class="field" bind:value={tryStatement} spellcheck="false" placeholder="A statement about the text, answered yes or no" />
        {#if tryAnswer}
          {@const yes = tryAnswer.answers.check?.noul}
          <div class="result">
            {typeof yes === "number" ? `${Math.round(yes * 100)}% yes` : JSON.stringify(tryAnswer.answers)}
            <span class="dim">· {tryAnswer.model} · {tryAnswer.ms} ms · {tryAnswer.input_tokens + tryAnswer.output_tokens} tokens</span>
          </div>
        {/if}
      </div>
      <div class="control"><button class="btn small" disabled={asking || !s?.key} onclick={() => ask()}>{asking ? "Asking…" : "Ask"}</button></div>
    </div>
  </section>

  <h2>Hub's own use</h2>
  <section>
    <p class="intro">
      Asked when Hub would guess. Falls back if it is slow.
    </p>
    {#each data?.features ?? [] as f (f.name)}
      <div class="line">
        <div class="what">
          <div class="name">{FEATURE_NAMES[f.name] ?? f.name}</div>
          <div class="hint">{f.what}.</div>
        </div>
        <div class="control">
          <Switch label={FEATURE_NAMES[f.name] ?? f.name} on={featureOn(f.name)} disabled={!s?.key} onchange={(v) => setFeature(f.name, v)} />
        </div>
      </div>
    {/each}
  </section>

  {#if mine.length && apis.tune}
    {@const t = apis.tune}
    <h2>
      Suggestions
      <span class="dim">
        {#if t.running}tuning now, reading {c.label}'s decisions in the background{:else if t.at}from the tune at {clock(t.at)} · read {t.read} of {t.decisions} decisions in full{:else}a tune reads what {c.label} decided and suggests how to make it better{/if}
      </span>
      {#if !t.running}<button class="btn small tune" onclick={() => apis.runTune().catch(hub.fail)}>{t.at ? "Tune again" : "Tune"}</button>{/if}
    </h2>
    {#if t.error && !t.running}<p class="intro err">{t.error}</p>{/if}
    {#if apis.open.length}
      <section class="list">
        {#each apis.open as sg (sg.id)}
          <div class="sg">
            <div class="sg-top">
              <span class="pill kind {sg.kind}">{KINDS[sg.kind] ?? sg.kind}</span>
              <span class="when">{FEATURE_NAMES[sg.feature] ?? sg.feature}</span>
            </div>
            <p class="sg-text">{sg.text}</p>
            <div class="sg-act">
              <span class="when" data-tip={sg.backing.map((at) => clock(at)).join(", ")}>{sg.backing.length} decision{sg.backing.length === 1 ? "" : "s"}</span>
              <button class="btn small" onclick={() => apis.answer(sg.id, false).catch(hub.fail)}>Skip</button>
              {#if sg.kind === "threshold"}
                <button class="btn small primary" data-tip="Sets the threshold now" onclick={() => apis.answer(sg.id, true).catch(hub.fail)}>Use {Math.round((sg.sure ?? 0) * 100)}%</button>
              {:else}
                <button class="btn small primary" data-tip="Opens a thread in Hub's project that makes this change" onclick={() => build(c.label, sg)}>Build</button>
              {/if}
            </div>
          </div>
        {/each}
      </section>
    {:else if t.at && !t.running && !t.error}
      <p class="intro">Nothing left to look at from that tune.</p>
    {/if}
  {/if}
  {#if mine.length}
    {@const autos = mine.filter((d) => d.feature === "auto")}
    <h2>
      What it decided
      <span class="dim">
        {mine.length} answers · went by {pct(mine.filter((d) => d.acted).length, mine.length)}%{#if autos.length} · agreed with Auto's word lists {pct(autos.filter((d) => d.agree).length, autos.length)}%{/if}
      </span>
    </h2>
    <section class="list">
      {#each mine.slice(0, logShown) as d (`${d.at}:${d.feature}:${d.thread}`)}
        <div class="change">
          <span class="pill">{FEATURE_NAMES[d.feature] ?? d.feature}</span>
          <span class="subject" data-tip={d.text}>{d.text}</span>
          {#if d.feature === "auto" && !d.agree}<span class="pill">Words: {d.hub}</span>{/if}
          <span class="pill" class:warn={d.feature === "auto" && !d.agree} class:dim={!d.acted} data-tip={d.acted ? "Hub went by it" : "Too unsure; Hub did what it did before"}>
            {SAID[d.said] ?? d.said}{d.confidence != null ? ` ${Math.round(d.confidence * 100)}%` : ""}
          </span>
          {#if d.thread}<button class="btn small" onclick={() => hub.open(d.thread!)}>Thread</button>{/if}
          <span class="when">{ago(d.at)}</span>
        </div>
      {/each}
    </section>
    {#if mine.length > logShown}<button class="btn small more" onclick={() => (logShown += 100)}>Show {Math.min(100, mine.length - logShown)} more</button>{/if}
  {/if}

<style>
  .more { margin-top: 8px; }
  h2 { color: var(--text); font-size: 15px; font-weight: 600; margin: 30px 0 4px; }
  h2 .dim { font-weight: 400; font-size: 13px; margin-left: 6px; }
  .line { display: flex; align-items: center; gap: 32px; padding: 14px 0; }
  .line + .line { border-top: 1px solid var(--line); }
  .line.top { align-items: flex-start; }
  .what { flex: 1; min-width: 0; display: flex; flex-direction: column; gap: 4px; }
  .name { color: var(--text); font-size: 14px; }
  .hint { color: var(--muted); font-size: 13px; }
  .control { display: flex; align-items: center; gap: 8px; flex: none; }
  .control :global(.select.cap) { min-width: 150px; }
  .dim { color: var(--muted); }
  .field { width: 100%; height: 28px; padding: 0 9px; background: var(--bg); border: 1px solid var(--line-strong); border-radius: var(--r-md); font-size: 13px; outline: none; color: var(--text); }
  .field:focus { border-color: var(--accent); }
  .field.area { height: auto; padding: 6px 9px; resize: vertical; font-family: inherit; }
  .result { font-size: 13px; padding: 6px 10px; border-radius: var(--r-md); color: var(--green); background: var(--green-bg); }
  .change { display: flex; align-items: center; gap: 10px; padding: 8px 0; min-height: 36px; }
  .change + .change { border-top: 1px solid var(--line); }
  .subject { flex: 1; min-width: 0; color: var(--text-2); overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .when { color: var(--muted); font-size: 12px; flex: none; }
  .pill { font-size: 12px; color: var(--text-3); background: var(--surface-hover); border-radius: var(--r-sm); padding: 1px 7px; white-space: nowrap; }
  .pill.warn { color: var(--amber); background: var(--amber-bg); }
  .pill.dim { background: transparent; border: 1px solid var(--line); }
  .intro { color: var(--text-3); font-size: 13px; line-height: 1.5; margin: 2px 0 6px; }
  .btn.small { height: 24px; padding: 0 9px; font-size: 12px; }
  h2 .tune { margin-left: 10px; vertical-align: 1px; }
  .err { color: var(--red); }
  .sg { padding: 10px 0; }
  .sg + .sg { border-top: 1px solid var(--line); }
  .sg-top { display: flex; align-items: center; gap: 8px; }
  .sg-text { color: var(--text-2); font-size: 13px; line-height: 1.5; margin: 5px 0 6px; }
  .sg-act { display: flex; align-items: center; gap: 8px; }
  .sg-act .when { margin-right: auto; }
  .pill.kind.context { color: color-mix(in srgb, var(--accent) 85%, var(--text)); background: var(--accent-bg); }
  .pill.kind.threshold { color: var(--amber); background: var(--amber-bg); }
  .pill.kind.wording { color: var(--green); background: var(--green-bg); }
  :global(.branded) h2 { display: flex; align-items: center; gap: 8px; }
  :global(.branded) h2::before { content: ""; width: 6px; height: 6px; border-radius: 2px; background: var(--brand); }
</style>
