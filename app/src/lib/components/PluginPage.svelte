<!-- One plugin's own page: a banner with the owner's picture and Customs' stamp, the numbers (stars over 30 days, this week, last commit, people), what Hub found inside the repository, the README, Hub's first look before Customs reviews it, facts and similar plugins.
     Used by PluginsView.svelte. -->
<script lang="ts">
  import { quiet } from "../quiet";
  // One plugin's own page: a banner with the owner's picture and Customs' stamp, the numbers
  // (stars over 30 days, this week, last commit, people), what Hub found inside the repository,
  // the README, Hub's first look before Customs reviews it, facts and similar plugins.
  import { api } from "../api";
  import type { PluginDetail } from "../bindings/PluginDetail";
  import type { WorkerReady } from "../bindings/WorkerReady";
  import { loadTakers } from "../ownTools.svelte";
  import { hubdHas, tooOld } from "../compat";
  import { ago } from "../format";
  import { openUrl } from "../native";
  import { count, customs, daysAgo, kindLabel, line, spanLabel, weekLabel } from "../plugins";
  import { hub } from "../state.svelte";
  import Icon from "./Icon.svelte";
  import Markdown from "./Markdown.svelte";

  // `side`: shown in a panel beside the Discover lists on a wide window, with a close button
  // instead of the way back.
  let { name, side = false }: { name: string; side?: boolean } = $props();

  let d = $state<PluginDetail | null>(null);
  let failed = $state<string | null>(null);
  let wholeReadme = $state(false);
  // A Worker: installing it here, and what its ready check said.
  let installing = $state(false);
  let ready = $state<WorkerReady | null>(null);
  let installFailed = $state<string | null>(null);

  async function install(again = false) {
    if (!d?.worker) return;
    installing = true;
    installFailed = null;
    try {
      const folder = d.worker.installed;
      ready = again && folder ? await api.workerReady(folder) : await api.installWorker(d.repo.full_name);
      d.worker.installed = ready.folder;
      void loadTakers();
    } catch (e) {
      installFailed = e instanceof Error ? e.message : String(e);
    } finally {
      installing = false;
    }
  }

  // Customs' review of this repository: the page makes its entry on Customs' list (kind
  // `github-repo`) and asks for the review. A hubd from before Customs could read repositories
  // answers with a stamp it cannot honour, so the button says it needs the next version.
  const KIND = "github-repo";
  let reviewing = $state(false);
  let reviewNote = $state<string | null>(null);
  let reviewBad = $state(false);
  let canReview = $state(true);
  quiet(hubdHas("customs.github").then((v) => (canReview = v)), "hubd has"); // compat-until: 2026-12-05

  /** Puts the entry on the list when it is not there, then runs the review and shows its word. */
  async function review() {
    if (!repo || reviewing) return;
    const want = name;
    reviewing = true;
    reviewBad = false;
    reviewNote = null;
    try {
      if (!repo.customs) await api.editCustoms({ kind: KIND, name: repo.full_name, status: "held", reason: "Waiting for review" });
      const item = await api.reviewCustoms(KIND, repo.full_name);
      if (want !== name || !d) return;
      if (item) {
        d.repo.customs = item.status;
        d.repo.customs_reason = item.reason;
        reviewNote = item.reason || null;
      }
    } catch (e) {
      if (want !== name) return;
      reviewBad = true;
      if (tooOld(e)) {
        canReview = false;
        reviewNote = "This Hub cannot review repositories yet. It needs the next version.";
      } else {
        reviewNote = e instanceof Error ? e.message : String(e);
      }
    } finally {
      if (want === name) reviewing = false;
    }
  }

  // A review that is already running (started by an agent or in another window) shows as running.
  async function watchRunning(want: string, full: string) {
    for (let i = 0; i < 120 && want === name; i++) {
      const a = await quiet(api.customsActivity(), "customs activity", null);
      if (!a || !a.reviewing.some((r) => r.kind === KIND && r.name === full)) break;
      reviewing = true;
      await new Promise((r) => setTimeout(r, 3000));
    }
    if (want !== name) return;
    if (reviewing) {
      reviewing = false;
      api.pluginRepo(want).then((r) => want === name && (d = r), () => {});
    }
  }

  $effect(() => {
    const want = name;
    d = null;
    failed = null;
    ready = null;
    installFailed = null;
    reviewing = false;
    reviewNote = null;
    reviewBad = false;
    api.pluginRepo(want).then(
      (r) => {
        if (want !== name) return;
        d = r;
        void watchRunning(want, r.repo.full_name);
      },
      (e) => want === name && (failed = e instanceof Error ? e.message : String(e)),
    );
  });

  const repo = $derived(d?.repo ?? null);
  const stamp = $derived(repo ? customs(repo) : null);
  const week = $derived(repo ? weekLabel(repo) : null);
  const chart = $derived(d ? line(d.curve, 260, 46, 2) : "");
  const area = $derived(chart ? `M0,46 L${chart.replaceAll(" ", " L")} L260,46 Z` : "");
  const STAMP_WORD = { ok: "Cleared", held: "Held", no: "Refused", none: "Not checked" } as const;

  // Without a picture of its own, the banner is the repository's name as a hex dump: a quiet
  // pattern that differs per plugin and fits a page about code.
  const dump = $derived.by(() => {
    const text = `${name} · ${repo?.description ?? ""} `.repeat(6);
    const rows: { off: string; hex: string; ascii: string }[] = [];
    for (let i = 0; i < 9; i++) {
      const chunk = [...text.slice(i * 16, i * 16 + 16).padEnd(16, ".")];
      rows.push({
        off: (i * 16).toString(16).padStart(8, "0"),
        hex: chunk.map((c) => (c.charCodeAt(0) & 0xff).toString(16).padStart(2, "0")).join(" "),
        ascii: chunk.map((c) => (/[\x20-\x7e]/.test(c) ? c : ".")).join(""),
      });
    }
    return rows;
  });

  function ask() {
    if (!repo) return;
    hub.startDraft(
      hub.draftFolder(),
      `Look at ${repo.url} and tell me in a few lines what it does, whether it would be useful for my work, and what it would need to run. Do not install or run anything.`,
    );
  }
</script>

<div class="page" class:side>
  <nav class="crumb">
    {#if side}
      <b>{name}</b>
      <button class="close" aria-label="Close" data-tip="Close (Esc)" onclick={() => hub.openPlugins()}><Icon name="x" size={14} /></button>
    {:else}
      <button onclick={() => hub.openPlugins()}>Plugins</button><span>›</span><b>{name}</b>
    {/if}
  </nav>

  {#if failed}
    <p class="say bad">{failed}</p>
  {:else if !d || !repo || !stamp}
    <div class="hero ghost"></div>
    <div class="stats">{#each Array(4) as _, i (i)}<div class="tile ghost"></div>{/each}</div>
  {:else}
    <section class="hero">
      <div class="banner" class:picture={!!d.banner}>
        {#if d.banner}
          <img src={d.banner} alt="" />
        {:else}
          <div class="hex" aria-hidden="true">{#each dump as r (r.off)}<div><span class="o">{r.off}</span>  {r.hex}  <i>{r.ascii}</i></div>{/each}</div>
        {/if}
        <div class="stamp {stamp.tone}" data-tip={stamp.tone === "none" ? "Not reviewed yet: press Review" : `Customs: ${stamp.text}`}>
          CUSTOMS<span class="big">{STAMP_WORD[stamp.tone]}</span>{repo.archived ? "ARCHIVED" : kindLabel(repo.kind).toUpperCase()}
        </div>
      </div>
      <div class="ident">
        {#if repo.avatar}<img class="logo" src={repo.avatar} alt="" />{:else}<span class="logo"><Icon name="puzzle" size={30} /></span>{/if}
        <div class="idt">
          <h1>{repo.name}</h1>
          <div class="by">
            <span>{repo.owner}</span>
            {#if repo.license}<span>·</span><span>{repo.license}</span>{/if}
            {#if repo.language}<span>·</span><span>{repo.language}</span>{/if}
          </div>
        </div>
        <div class="acts">
          <button class="btn" onclick={() => openUrl(repo.url)}><Icon name="external" size={12} />Open on GitHub</button>
          <button class="btn" onclick={ask} data-tip="Starts a draft on Home that asks an agent to read it; nothing is sent until you do">Ask a thread about it</button>
          {#if d.worker?.installed}
            <button class="btn" disabled={installing} onclick={() => install(true)} data-tip="Runs its ready check again">{installing ? "Checking…" : "Check again"}</button>
          {:else if d.worker}
            <button class="btn primary" disabled={installing || !!d.worker.why_not || !hub.connected} data-tip={d.worker.why_not ?? "Clones it into your projects folder and checks it can run here"} onclick={() => install()}>{installing ? "Installing…" : "Install"}</button>
          {:else}
            <button class="btn primary" disabled data-tip="Installing, with Customs' review, comes in the next step">Install</button>
          {/if}
        </div>
      </div>
      {#if repo.description}<p class="tagline">{repo.description}</p>{/if}
      {#if d.worker}
        <div class="worker">
          {#if d.worker.installed}<p class="ok">Installed in <span class="mono">{d.worker.installed}</span>.</p>
          {:else if d.worker.why_not}<p class="muted">{d.worker.why_not}</p>
          {:else}<p class="muted">A Worker: does whole jobs by itself.</p>{/if}
          {#if installFailed}<p class="bad">{installFailed}</p>{/if}
          {#if ready}
            {#if ready.error}<p class="bad">Its ready check could not run: {ready.error}</p>
            {:else if ready.ok != null}
              <p class={ready.ok ? "ok" : "bad"}>{ready.ok ? "Ready to run here." : "Not ready yet. What is missing:"}</p>
              <ul class="checks">
                {#each ready.checks.filter((c) => !ready!.ok ? c.status !== "pass" : c.status === "warn") as c, i (i)}
                  <li class="is-{c.status}"><span class="st">{c.status}</span><span>{c.label}{c.detail ? `: ${c.detail}` : ""}</span></li>
                {/each}
              </ul>
            {/if}
          {/if}
        </div>
      {/if}
      <div class="pills">
        <span class="pill k">{kindLabel(repo.kind)}</span>
        {#each repo.topics.slice(0, 8) as t (t)}<span class="pill">{t}</span>{/each}
      </div>
    </section>

    <div class="stats">
      <div class="tile wide">
        <div class="l">Stars{chart ? `, last ${spanLabel(d.curve_span)}` : ""}</div>
        <div class="chart-row">
          <div class="v">{count(repo.stars)}</div>
          {#if chart}
            <svg viewBox="0 0 260 46" preserveAspectRatio="none" aria-hidden="true">
              <defs><linearGradient id="fill" x1="0" x2="0" y1="0" y2="1"><stop offset="0" stop-color="var(--green)" stop-opacity=".32" /><stop offset="1" stop-color="var(--green)" stop-opacity="0" /></linearGradient></defs>
              <path d={area} fill="url(#fill)" /><polyline points={chart} />
            </svg>
          {/if}
        </div>
      </div>
      <div class="tile">
        <div class="l">This week</div>
        <div class="v" class:gain={!!repo.week}>{week ?? "–"}</div>
        <div class="s">{repo.week == null ? "too many stars to count" : repo.week_estimated ? "projected from the newest stars" : "stars in the last 7 days"}</div>
      </div>
      <div class="tile">
        <div class="l">Last commit</div>
        <div class="v">{repo.pushed_at ? (ago(repo.pushed_at) === "now" ? "now" : `${ago(repo.pushed_at)} ago`) : "–"}</div>
        <div class="s">{d.commits_month >= 100 ? "100+" : d.commits_month} {d.commits_month === 1 ? "commit" : "commits"} this month</div>
      </div>
      <div class="tile">
        <div class="l">People</div>
        <div class="v">{d.contributors >= 100 ? "100+" : d.contributors}</div>
        <div class="s">{count(d.forks)} {d.forks === 1 ? "fork" : "forks"}</div>
      </div>
    </div>

    <div class="cols">
      <div class="main">
        <section>
          <h3>What's inside <span>what Hub found at the top of the repository</span></h3>
          {#if d.inside.length}
            <div class="inside">
              {#each d.inside as line (line)}<div class="part"><Icon name="file" size={13} /><span>{line}</span></div>{/each}
            </div>
          {:else}
            <p class="say">See the README.</p>
          {/if}
        </section>

        <section>
          <h3>Readme</h3>
          {#if d.readme.trim()}
            <div class="readme" class:whole={wholeReadme}><Markdown text={d.readme} /></div>
            {#if !wholeReadme}<button class="more" onclick={() => (wholeReadme = true)}>Show the whole readme</button>{/if}
          {:else}
            <p class="say">No README.</p>
          {/if}
        </section>
      </div>

      <aside>
        <div class="card">
          <div class="t">Customs <span class="st {stamp.tone}">{stamp.text}</span></div>
          {#each d.checks as c (c.text)}
            <div class="chk"><span class="m {c.level}">{c.level === "ok" ? "✓" : "!"}</span>{c.text}</div>
          {/each}
          <div class="rev">
            <button class="btn" disabled={reviewing || !canReview || !hub.connected} data-tip={canReview ? "Customs reads the repository and clears, holds or refuses it" : "Needs the next version of Hub"} onclick={review}>
              {reviewing ? "Reviewing…" : stamp.tone === "none" ? "Review" : "Review again"}
            </button>
            {#if !canReview}<span class="muted">Needs the next version</span>{/if}
          </div>
          {#if reviewing}
            <p class="note">Customs is reading it.</p>
          {:else if reviewNote}
            <p class="note" class:bad={reviewBad}>{reviewNote}</p>
          {:else if stamp.tone === "none"}
            <p class="note">A first look. Review sends it through Customs.</p>
          {/if}
        </div>
        <div class="card">
          <div class="t">Facts</div>
          <div class="kv"><span>Made</span><span>{daysAgo(repo.created_at)} ago</span></div>
          {#if repo.language}<div class="kv"><span>Language</span><span>{repo.language}</span></div>{/if}
          <div class="kv"><span>Licence</span><span>{repo.license ?? "none"}</span></div>
          {#if d.homepage}<div class="kv"><span>Website</span><button class="link" onclick={() => d?.homepage && openUrl(d.homepage)}>{d.homepage.replace(/^https?:\/\//, "").replace(/\/$/, "")}</button></div>{/if}
        </div>
        {#if d.similar.length}
          <div class="card">
            <div class="t">Similar</div>
            {#each d.similar as s (s.full_name)}
              <button class="sim" onclick={() => hub.openPlugins(s.full_name)}>
                {#if s.avatar}<img class="mini" src={s.avatar} alt="" />{:else}<span class="mini"></span>{/if}
                <span class="sn">{s.name}<span class="sd">{kindLabel(s.kind)} · ★ {count(s.stars)} · {customs(s).text.toLowerCase()}</span></span>
              </button>
            {/each}
          </div>
        {/if}
      </aside>
    </div>
  {/if}
</div>

<style>
  .page { flex: 1; min-height: 0; overflow-y: auto; padding: 18px var(--gutter) 60px; width: 100%; container-type: inline-size; }
  .page > * { max-width: 1200px; }
  .crumb { display: flex; gap: 6px; align-items: center; color: var(--muted); font-size: 13px; }
  .crumb button { color: var(--muted); }
  .crumb button:hover { color: var(--text); text-decoration: underline; text-underline-offset: 3px; }
  .crumb b { color: var(--text-3); font-weight: 400; }
  .side { padding: 14px 22px 60px; }
  .side .crumb b { flex: 1; min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .crumb .close { display: grid; padding: 4px; border-radius: var(--r-sm); }
  .crumb .close:hover { background: var(--surface-hover); text-decoration: none; }
  .say { color: var(--text-3); font-size: 13px; margin: 6px 0; }
  .say.bad { color: var(--red); margin-top: 16px; }
  .note { color: var(--muted); font-size: 12px; margin: 8px 0 0; }
  .note.bad { color: var(--red); }
  .rev { display: flex; gap: 10px; align-items: center; margin-top: 10px; }
  .rev .muted { color: var(--muted); font-size: 12px; }
  .btn { height: 30px; padding: 0 12px; font-size: 13px; display: inline-flex; align-items: center; gap: 6px; }
  .ghost { }

  /* The banner: the repository's own picture, or its name as a hex dump fading downwards. */
  .hero { margin-top: 14px; border-radius: var(--r-lg); overflow: hidden; border: 1px solid var(--line); background: var(--surface); }
  .hero.ghost { height: 300px; }
  .banner { height: 150px; position: relative; overflow: hidden;
    background: radial-gradient(600px 220px at 85% -20%, color-mix(in srgb, var(--accent) 28%, transparent), transparent 70%),
      radial-gradient(500px 200px at 10% 130%, color-mix(in srgb, var(--green) 14%, transparent), transparent 70%), #1d1c22; }
  .banner img { width: 100%; height: 100%; object-fit: cover; opacity: 0.55; }
  .banner.picture::after { content: ""; position: absolute; inset: 0; background: linear-gradient(transparent 30%, var(--surface)); }
  .hex { position: absolute; inset: 0; padding: 10px 16px; font: 11px/14px var(--mono); color: color-mix(in srgb, var(--accent) 18%, transparent); white-space: pre; letter-spacing: 0.5px;
    mask-image: linear-gradient(#000 35%, transparent 92%); -webkit-mask-image: linear-gradient(#000 35%, transparent 92%); }
  .hex i { font-style: normal; color: color-mix(in srgb, var(--accent) 50%, transparent); }
  .hex .o { color: color-mix(in srgb, var(--accent) 12%, transparent); }

  /* Customs' stamp, like the one in a passport: colour only says its state. */
  .stamp { position: absolute; right: 30px; top: 22px; z-index: 1; transform: rotate(-7deg); color: var(--muted); border: 2.5px dashed currentColor; border-radius: var(--r-lg); padding: 6px 14px 4px;
    text-align: center; font: 600 11px/1.25 var(--mono); letter-spacing: 2px; background: color-mix(in srgb, var(--bg) 55%, transparent); display: flex; flex-direction: column; align-items: center; }
  .stamp .big { font-size: 20px; letter-spacing: 4px; margin: 1px 0; text-transform: uppercase; }
  .stamp.ok, .stamp.held, .stamp.no { border-style: solid; box-shadow: inset 0 0 0 3px var(--bg), inset 0 0 0 4.5px currentColor; }
  .stamp.ok { color: var(--green); }
  .stamp.held { color: var(--amber); }
  .stamp.no { color: var(--red); }

  .ident { display: flex; gap: 18px; align-items: flex-end; padding: 0 24px 16px; margin-top: -38px; position: relative; }
  .logo { width: 76px; height: 76px; border-radius: var(--r-lg); flex: none; object-fit: cover; background: color-mix(in srgb, var(--accent) 12%, var(--surface-2)); border: 1px solid var(--line-strong);
    display: grid; place-items: center; color: var(--accent); box-shadow: 0 6px 24px rgba(0, 0, 0, 0.45); }
  .idt { flex: 1; min-width: 0; }
  h1 { font-size: 26px; font-weight: 600; letter-spacing: -0.2px; margin: 0; color: var(--text); overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .by { color: var(--muted); font-size: 13px; display: flex; gap: 8px; margin-top: 2px; }
  .acts { display: flex; gap: 8px; flex-wrap: wrap; justify-content: flex-end; }
  .tagline { font-size: 15px; color: var(--text-2); padding: 0 24px; margin: 0 0 14px; max-width: 760px; line-height: 1.5; }
  .pills { display: flex; gap: 6px; padding: 0 24px 20px; flex-wrap: wrap; }
  .pill { font-size: 12px; border: 1px solid var(--line-strong); border-radius: 999px; padding: 2px 10px; color: var(--text-3); }
  .pill.k { border-color: color-mix(in srgb, var(--accent) 35%, transparent); color: var(--accent); background: var(--accent-bg); }

  .stats { display: grid; grid-template-columns: 1.6fr 1fr 1fr 1fr; gap: 10px; margin-top: 14px; }
  .tile { background: var(--surface); border: 1px solid var(--line); border-radius: var(--r-lg); padding: 12px 14px; min-width: 0; min-height: 86px; }
  .tile .l { color: var(--muted); font-size: 12px; }
  .tile .v { font-size: 22px; font-weight: 600; margin-top: 2px; color: var(--text); white-space: nowrap; }
  .tile .v.gain { color: var(--green); }
  .tile .s { color: var(--muted); font-size: 12px; }
  .chart-row { display: flex; align-items: flex-end; gap: 12px; }
  .chart-row svg { flex: 1; height: 46px; min-width: 0; }
  .chart-row polyline { fill: none; stroke: var(--green); stroke-width: 1.8; vector-effect: non-scaling-stroke; }

  .cols { display: grid; grid-template-columns: minmax(0, 1fr) calc(330px * var(--text-grow)); gap: 22px; margin-top: 22px; }
  @container (max-width: 820px) { .cols { grid-template-columns: 1fr; } }
  @container (max-width: 640px) { .stats { grid-template-columns: 1fr 1fr; } }
  section { margin-bottom: 26px; }
  h3 { font-size: 14px; font-weight: 600; margin: 0 0 10px; color: var(--text); display: flex; gap: 10px; align-items: baseline; }
  h3 span { color: var(--muted); font-weight: 400; font-size: 12px; }
  .inside { display: grid; grid-template-columns: repeat(auto-fill, minmax(240px, 1fr)); gap: 8px; }
  .part { display: flex; gap: 8px; align-items: center; background: var(--surface); border: 1px solid var(--line); border-radius: var(--r-md); padding: 10px 12px; color: var(--text-2); font-size: 13px; }
  .part :global(svg) { color: var(--muted); flex: none; }
  .readme { background: var(--surface); border: 1px solid var(--line); border-radius: var(--r-lg); padding: 6px 18px; font-size: 13px; color: var(--text-2); position: relative; max-height: 320px; overflow: hidden; }
  .readme:not(.whole)::after { content: ""; position: absolute; left: 0; right: 0; bottom: 0; height: 80px; background: linear-gradient(transparent, var(--surface)); }
  .readme.whole { max-height: none; }
  .more { color: var(--accent); font-size: 13px; margin-top: 6px; }
  .more:hover { text-decoration: underline; }

  .card { background: var(--surface); border: 1px solid var(--line); border-radius: var(--r-lg); padding: 14px 16px; margin-bottom: 12px; }
  .card .t { font-weight: 600; font-size: 13px; margin-bottom: 8px; display: flex; justify-content: space-between; align-items: baseline; color: var(--text); }
  .st { font-weight: 400; font-size: 12px; color: var(--muted); }
  .st.ok { color: var(--green); }
  .st.held { color: var(--amber); }
  .st.no { color: var(--red); }
  .chk { display: flex; gap: 8px; font-size: 13px; color: var(--text-3); padding: 3px 0; }
  .m { width: 14px; flex: none; text-align: center; }
  .m.ok { color: var(--green); }
  .m.warn { color: var(--amber); }
  .kv { display: flex; justify-content: space-between; gap: 12px; font-size: 13px; padding: 3px 0; color: var(--text-2); }
  .kv > span:first-child { color: var(--muted); }
  .link { color: var(--accent); font-size: 13px; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .link:hover { text-decoration: underline; }
  .sim { display: flex; gap: 10px; align-items: center; padding: 6px 0; width: 100%; text-align: left; }
  .sim:hover .sn { color: var(--text); }
  .mini { width: 28px; height: 28px; border-radius: var(--r-md); flex: none; border: 1px solid var(--line-strong); background: var(--surface-hover); object-fit: cover; }
  .sn { display: flex; flex-direction: column; font-size: 13px; color: var(--text-2); min-width: 0; }
  .sd { color: var(--muted); font-size: 12px; }
  .worker { margin-top: 10px; display: flex; flex-direction: column; gap: 6px; font-size: 13px; }
  .worker p { margin: 0; line-height: 1.5; }
  .worker .ok { color: var(--green); }
  .worker .bad { color: var(--red); }
  .worker .muted { color: var(--muted); }
  .checks { list-style: none; margin: 0; padding: 0; display: flex; flex-direction: column; gap: 3px; }
  .checks li { display: flex; gap: 8px; color: var(--text-3); }
  .checks .st { width: 36px; flex: none; font-size: 12px; color: var(--muted); }
  .checks .is-fail .st { color: var(--red); }
  .checks .is-warn .st { color: var(--amber); }
</style>
