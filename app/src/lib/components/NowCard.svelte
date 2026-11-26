<!-- Now: what is going on right now, live. The threads that work, each with a clock that runs by itself, and the ones that wait on you first, with the question and Approve / Deny in the card when it is a plain one (a risky one only offers Open, where the page says what it would hit). Everything comes from the stream the window already has; nothing is asked for on a timer.
     Used by CardBody.svelte. -->
<script lang="ts">
  import { poll } from "../poll";
  import Quiet from "./Quiet.svelte";
  // Now: what is going on right now, live. The threads that work, each with a clock that runs by
  // itself, and the ones that wait on you first, with the question and Approve / Deny in the card
  // when it is a plain one (a risky one only offers Open, where the page says what it would hit).
  // Everything comes from the stream the window already has; nothing is asked for on a timer.
  import { api } from "../api";
  import { folderName, projectFolder } from "../format";
  import { hub } from "../state.svelte";
  import { folded, type PermissionItem } from "../transcript";
  import HowCounted from "./HowCounted.svelte";

  let now = $state(Date.now());
  $effect(() => {
    const t = poll(() => (now = Date.now()), 1000);
    return () => t();
  });

  const live = $derived(hub.everything.filter((t) => (t.status === "running" || t.status === "waiting_on_you") && !t.settled_at));
  const asking = $derived(live.filter((t) => t.status === "waiting_on_you").sort((a, b) => a.updated_at - b.updated_at));
  const working = $derived(live.filter((t) => t.status === "running").sort((a, b) => (a.sent_at ?? a.created_at) - (b.sent_at ?? b.created_at)));

  // The log of a thread that waits is read so its question can show; following it keeps it coming.
  $effect(() => {
    for (const t of asking) if (!hub.events[t.id]) hub.follow(t.id);
  });
  /** The question a thread waits on, if it is one the log has. */
  function question(id: string): PermissionItem | null {
    const log = hub.events[id];
    if (!log) return null;
    const open = folded(log).filter((i): i is PermissionItem => i.kind === "permission" && !i.resolved);
    return open.at(-1) ?? null;
  }

  const clock = (since: number) => {
    const s = Math.max(0, Math.floor((now - since) / 1000));
    const h = Math.floor(s / 3600);
    const m = Math.floor((s % 3600) / 60);
    const p = (n: number) => String(n).padStart(2, "0");
    return h ? `${h}:${p(m)}:${p(s % 60)}` : `${m}:${p(s % 60)}`;
  };
  const where = (cwd: string) => folderName(projectFolder(cwd));

  async function answer(id: string, q: PermissionItem, kind: "allow" | "reject") {
    const o = q.options.find((x) => x.kind.startsWith(kind) && (kind === "reject" || x.kind === "allow_once")) ?? q.options.find((x) => x.kind.startsWith(kind));
    if (!o) return hub.open(id);
    try {
      await api.answer(id, q.id, o.optionId);
    } catch (e) {
      hub.fail(e);
    }
  }
</script>

<HowCounted card="now">
  <p>Threads working or waiting on you, on every machine, as they change.</p>
</HowCounted>

{#if !live.length}
  <Quiet text="Nothing running" />
{:else}
  <div class="list">
    {#each asking as t (t.id)}
      {@const q = question(t.id)}
      <div class="ask">
        <button class="line" onclick={() => hub.open(t.id)}>
          <span class="dot waiting_on_you"></span>
          <span class="t">{t.title}</span>
          <span class="where">{where(t.cwd)}{t.host ? ` · ${t.host}` : ""}</span>
          <span class="clock amber">{clock(t.updated_at)}</span>
        </button>
        {#if q}
          <div class="q">{q.command ?? q.title.split("\n")[0]}</div>
          <div class="acts">
            {#if !q.risk}<button class="go" onclick={() => answer(t.id, q, "allow")}>Approve</button><button onclick={() => answer(t.id, q, "reject")}>Deny</button>{/if}
            <button class="open" onclick={() => hub.open(t.id)}>Open</button>
          </div>
        {/if}
      </div>
    {/each}
    {#each working as t (t.id)}
      <button class="line row" onclick={() => hub.open(t.id)}>
        <span class="dot running"></span>
        <span class="t">{t.title}</span>
        <span class="where">{where(t.cwd)}{t.host ? ` · ${t.host}` : ""}</span>
        <span class="clock">{clock(t.sent_at ?? t.created_at)}</span>
      </button>
    {/each}
  </div>
  <p class="caption"><em>{working.length}</em> working · <em>{asking.length}</em> {asking.length === 1 ? "needs" : "need"} you</p>
{/if}

<style>
  .list { display: flex; flex-direction: column; }
  .line { display: flex; align-items: center; gap: 10px; width: 100%; min-width: 0; text-align: left; padding: 0; }
  .row { height: 38px; border-bottom: 1px solid var(--line); }
  .row:last-child { border-bottom: 0; }
  .t { flex: 1; min-width: 0; white-space: nowrap; overflow: hidden; text-overflow: ellipsis; font-size: 13px; color: var(--text); }
  .where { color: var(--muted); font-size: 12px; white-space: nowrap; max-width: 34%; overflow: hidden; text-overflow: ellipsis; }
  .clock { font-family: var(--mono); font-size: 12px; color: var(--text-3); min-width: 52px; text-align: right; font-variant-numeric: tabular-nums; }
  .clock.amber { color: var(--amber); }
  .ask { background: var(--amber-bg); border: 1px solid color-mix(in srgb, var(--amber) 30%, transparent); border-radius: var(--r-md); padding: 8px 11px; margin-bottom: 6px; }
  .q { font-family: var(--mono); font-size: 12px; color: var(--text-2); background: var(--bg); border-radius: var(--r-sm); padding: 4px 8px; margin: 8px 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .acts { display: flex; gap: 6px; }
  .acts button { height: 26px; padding: 0 11px; border-radius: var(--r-sm); background: var(--surface-2); border: 1px solid var(--line-strong); color: var(--text-2); font-size: 12px; }
  .acts .go { background: var(--accent); border-color: var(--accent); color: var(--bg); font-weight: 600; }
  .acts .open { border-color: transparent; background: none; color: var(--accent); }
</style>
