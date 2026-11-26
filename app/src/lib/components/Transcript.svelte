<!-- The chat itself: draws a thread's items (messages, activity lines, reviews, breaks like "New") as one list. It receives built items; it does not fetch them.
     Used by ThreadView.svelte. -->
<script lang="ts">
  import { poll } from "../poll";
  import { clock, folderName } from "../format";
  import type { ImageItem, Item } from "../transcript";
  import { ago, later, short, when, hits } from "../transcriptWords";
  import Icon from "./Icon.svelte";
  import UserMessage from "./UserMessage.svelte";
  import RewoundNote from "./RewoundNote.svelte";
  import ActivityRow from "./ActivityRow.svelte";
  import WebRow from "./WebRow.svelte";
  import Working from "./Working.svelte";
  import IconSwap from "./IconSwap.svelte";
  import { menu } from "../menu.svelte";
  import { askUndo, messageMenu } from "../menus.svelte";
  import type { Item as LogItem, ReviewItem } from "../transcript";
  import { group, worked, type Shown, type Step } from "../activity";
  import StreamingText from "./StreamingText.svelte";
  import Changes from "./Changes.svelte";
  import Permission from "./Permission.svelte";
  import Receipt from "./Receipt.svelte";
  import RuleChip from "./RuleChip.svelte";
  import ReviewCard from "./ReviewCard.svelte";
  import DesignCard from "./DesignCard.svelte";
  import Checkpoint from "./Checkpoint.svelte";
  import DocsStamp from "./DocsStamp.svelte";
  import ProofStamp from "./ProofStamp.svelte";
  import ImageCarousel from "./ImageCarousel.svelte";
  import { hub } from "../state.svelte";
  import { api } from "../api";
  import { openUrl } from "../native";
  import { engineProvider } from "../providers.svelte";
  import ProviderMark from "./ProviderMark.svelte";
  import Break from "./Break.svelte";
  import { GLYPHS, machineGlyph } from "../refs";
  import PluginChatCard from "./PluginChatCard.svelte";
  import HandoffOfferCard from "./HandoffOfferCard.svelte";
  import AskLine from "./AskLine.svelte";
  import PageCard from "./PageCard.svelte";
  import ChildLine from "./ChildLine.svelte";
  import TaskLine from "./TaskLine.svelte";
  import HandoverHint from "./HandoverHint.svelte";
  import DeskNote from "./DeskNote.svelte";
  import CustomsAsk from "./CustomsAsk.svelte";
  import TerminalOutput from "./TerminalOutput.svelte";
  import { playfulWord, workedQuietly } from "../activity";
  import { siteTint, webAnswer, webAsked } from "../web";
  import { detailOf, prefs } from "../prefs.svelte";
  import { workShape } from "../workGrid";
  import WorkGrid from "./WorkGrid.svelte";
  import Steps from "./Steps.svelte";
  import { stepsShown, stepsLine, type PlanItem } from "../steps";
  import FocusNote from "./FocusNote.svelte";
  import TopicNote from "./TopicNote.svelte";
  import ErrorCard from "./ErrorCard.svelte";
  import WorkerRunCard from "./WorkerRun.svelte";
  import Reactions from "./Reactions.svelte";
  import EmojiPicker from "./EmojiPicker.svelte";
  import { emojis } from "../reactions.svelte";

  let {
    items,
    threadId,
    cwd,
    host = null,
    live = false,
    find = "",
  }: {
    items: Item[];
    threadId: string;
    cwd: string;
    /** The machine the thread runs on, by the name this one knows it under; none when it is here. */
    host?: string | null;
    /** The thread is working on a turn. */
    live?: boolean;
    /** What Ctrl F looks for: folded turns and closed steps that hold it open, so it can be found. */
    find?: string;
  } = $props();

  const shown = $derived(group(items));
  emojis.ensure();
  const needle = $derived(find.trim().toLowerCase());
  const hit = (s?: string | null) => hits(needle, s);
  const stepHit = (s: Step) => (s.kind === "thought" ? hit(s.text) : hit(s.title) || hit(s.detail));
  function found(it: Shown): boolean {
    if (!needle) return false;
    if (it.kind === "user" || it.kind === "assistant") return hit(it.text);
    if (it.kind === "plan") return it.entries.some((e) => hit(e.content));
    if (it.kind === "activity") return it.steps.some(stepHit);
    return false;
  }
  // Activity lines the user opened, by position.
  let open = $state<Record<number, boolean>>({});

  const rel = (path: string) => {
    const base = cwd.replace(/[\\/]+$/, "").replace(/\\/g, "/");
    const p = path.replace(/\\/g, "/");
    return p.startsWith(base + "/") ? p.slice(base.length + 1) : p;
  };
  /** The engine a thread is on when the wait before this one switched it over: it ran out too. */
  const ranOutToo = (item: Extract<LogItem, { kind: "wait" }>) => {
    const at = items.indexOf(item);
    const before = items.slice(0, Math.max(at, 0)).findLast((i) => i.kind === "wait");
    return before?.kind === "wait" && before.state === "switched" ? before.to : undefined;
  };
  /** An engine's provider by the name people use: Codex, not OpenAI. */
  const prov = (engine: string) => {
    const p = engineProvider(engine);
    return /codex/i.test(engine) ? { ...p, name: "Codex" } : p;
  };
  /** Limit waits whose offer to move to the other subscription was turned down here. */
  let waited = $state<Record<number, boolean>>({});
  const thisMachine = $derived(host || hub.localHost);

  const RESTARTED: Record<string, string> = {
    update: "Hub updated",
    restart: "Hub restarted for new settings",
    stop: "Hub was closed",
    crash: "Hub stopped unexpectedly",
    move: "The thread was moved to this machine",
  };

  // A finished turn folds into one line, as in T3 Code: the work, the plan, answered questions and
  // the text in between sit behind it; the final answer, its footer and Hub's own notes stay.
  // A turn runs from one message to the next.
  // The pictures of a turn show together, in one carousel where the first of them came.
  type Row =
    | { row: "item"; item: Shown; i: number; inner?: boolean }
    | { row: "fold"; key: number; label: string; failed: boolean; open: boolean; running: boolean }
    | { row: "word"; key: number; word: string }
    | { row: "steps"; key: number; plan: PlanItem }
    | { row: "stepline"; key: number; plan: PlanItem }
    | { row: "images"; key: number; images: ImageItem[] };
  let unfolded = $state<Record<number, boolean>>({});
  const turns = $derived.by(() => {
    const segs: number[][] = [];
    shown.forEach((item, i) => {
      if (item.kind === "user" || !segs.length) segs.push([]);
      segs[segs.length - 1].push(i);
    });
    return segs;
  });
  // How much of the work shows: this thread's level (its menu, Chat detail), else the default.
  const detail = $derived(detailOf(threadId));
  /** The steps card is up (the running turn made a plan): it holds the thread's one moving grid, so the live step below stands still. */
  const stepsUp = $derived(live && stepsShown(shown, detail) != null);
  /** The items of turns you asked as a web search: their search row stays out of the fold and
   *  their answer's sources show as cards. */
  const webTurn = $derived(
    new Set(turns.flatMap((seg) => {
      const first = shown[seg[0]];
      return first?.kind === "user" && webAsked(first.text) !== null ? seg : [];
    })),
  );
  const rows = $derived.by(() => {
    const out: Row[] = [];
    turns.forEach((seg, n) => {
      const images = seg.filter((i) => shown[i].kind === "image");
      const push = (i: number) => {
        const item = shown[i];
        if (item.kind !== "image") out.push({ row: "item", item, i });
        else if (i === images[0]) out.push({ row: "images", key: i, images: images.map((k) => shown[k] as ImageItem) });
      };
      const end = seg.find((i) => shown[i].kind === "turn");
      const last = n === turns.length - 1;
      // A turn is over when it ended, or when a message came after it (a turn cut off by a restart
      // or an update never gets its end, and should not stay spread out for ever).
      const running = live && last;
      const done = !running && (end != null || !last);
      const answer = seg.findLast((i) => shown[i].kind === "assistant");
      // While a turn runs, only what it does now stays out: its latest note and its latest step
      // (quiet: not even the note; minimal: nothing, a playful word says it works). The rest goes
      // into the line, which says how long it has worked.
      const lastStep = seg.findLast((i) => shown[i].kind === "activity");
      const searched = (i: number) => webTurn.has(i) && shown[i].kind === "activity" && !!(shown[i] as { web?: boolean }).web;
      const keep = (i: number) =>
        searched(i) || (running ? (detail !== "minimal" && i === lastStep) || (detail === "normal" && i === answer) : i === answer);
      const work = (i: number) => {
        const it = shown[i];
        return it.kind === "activity" || it.kind === "plan" || (it.kind === "permission" && it.resolved) || it.kind === "assistant";
      };
      // The steps card of the running turn sits above its live step (or last, with none), outside the
      // fold, and says how far the plan is: the plan itself is neither tucked nor shown where it was made.
      const card = running ? stepsShown(seg.map((i) => shown[i]), detail) : null;
      const cardPlan = (i: number) => !!card && shown[i].kind === "plan";
      const tucked = detail !== "all" && (done || running) ? seg.filter((i) => work(i) && !keep(i) && !cardPlan(i)) : [];
      const steps_row = () => card && out.push({ row: "steps", key: seg[0], plan: card });
      // Minimal: no line of work while it runs, only a word that changes with every new step.
      if (detail === "minimal" && running) {
        for (const i of seg) if (!tucked.includes(i) && !cardPlan(i)) push(i);
        const n = tucked.reduce((sum, i) => {
          const it = shown[i];
          return sum + (it.kind === "activity" ? it.steps.length : 1);
        }, 0);
        // With a plan the one line says where it is in the plan, in place of the word.
        if (card) out.push({ row: "stepline", key: seg[0], plan: card });
        else out.push({ row: "word", key: seg[0], word: playfulWord(threadId, n) });
        return;
      }
      if (!tucked.length) {
        for (const i of seg) {
          if (card && i === lastStep) steps_row();
          if (!cardPlan(i)) push(i);
        }
        if (card && lastStep == null) steps_row();
        return;
      }
      const first = shown[seg[0]];
      const stop = end != null ? shown[end] : undefined;
      const ms = first.kind === "user" ? (stop?.kind === "turn" ? stop.ts - first.ts : running ? tick - (first.receipt?.taken ?? first.ts) : null) : null;
      const steps = tucked.flatMap((i) => {
        const it = shown[i];
        return it.kind === "activity" ? it.steps : [];
      });
      const notes = tucked.filter((i) => shown[i].kind === "assistant").length;
      const changes = seg.map((i) => shown[i]).find((it) => it.kind === "changes");
      const plan = tucked.map((i) => shown[i]).findLast((it) => it.kind === "plan");
      const planned = plan?.kind === "plan" ? { done: plan.entries.filter((e) => e.status === "completed").length, of: plan.entries.length } : false;
      // Minimal and Quiet say how long, how far the plan got and what changed; what it read and ran is Normal's.
      const label =
        detail === "minimal" || detail === "quiet"
          ? workedQuietly(ms, changes?.kind === "changes" ? changes.files.length : 0, planned, running)
          : worked(steps, ms, card ? false : planned, notes, running);
      // Red only when the turn ended on a failure: a command that failed on the way (a probe for a
      // folder that is not there) and was gotten past is not the turn failing. It stays red inside.
      const failed = !running && steps.findLast((s) => s.kind === "tool")?.status === "failed";
      const open = unfolded[seg[0]] || tucked.some((i) => found(shown[i]));
      for (const i of seg) {
        if (card && i === lastStep) steps_row();
        // Quiet with the steps card up: the card says how far it is, so no "So far" line above it.
        if (i === tucked[0] && !(card && detail === "quiet" && !open)) out.push({ row: "fold", key: seg[0], label, failed, open, running });
        if (cardPlan(i)) continue;
        if (!tucked.includes(i)) push(i);
        else if (open) out.push({ row: "item", item: shown[i], i, inner: true });
      }
      if (card && lastStep == null) steps_row();
    });
    return out;
  });

  /** What the turn ending at `at` changed, and its last answer, for the reply's footer. */
  function replyAt(at: number) {
    let files: Extract<Shown, { kind: "changes" }> | undefined;
    let answer = "";
    for (let k = at - 1; k >= 0; k--) {
      const it = shown[k];
      if (it.kind === "user") break;
      if (it.kind === "changes") files ??= it;
      if (it.kind === "assistant" && !answer) answer = it.text;
    }
    return { files, answer };
  }

  // A running command's clock, ticking while the thread works.
  let tick = $state(Date.now());
  $effect(() => {
    if (!live) return;
    const timer = poll(() => (tick = Date.now()), 1000);
    return () => timer();
  });
  /** A terminal on the machine where the engine's sign-in ran out, signing it in there. */
  /** Sends the message a failed turn answered again, as it was. */
  function retryOf(item: Extract<LogItem, { kind: "error" }>): (() => void) | undefined {
    // The message the turn answered, or for an engine that never started, the last one sent before.
    const at = items.indexOf(item);
    const said =
      item.prompt != null
        ? items.findLast((i): i is Extract<LogItem, { kind: "user" }> => i.kind === "user" && i.seq === item.prompt)
        : items.slice(0, at).findLast((i): i is Extract<LogItem, { kind: "user" }> => i.kind === "user");
    if (!said || said.fromHub) return undefined;
    return () => api.send(threadId, said.text, said.attachments ?? []).catch((e) => hub.fail(e));
  }

    function signInOn(machine: string, engine: string, keep: boolean) {
    hub.openTerminal(machine, { what: keep ? "sign_in" : "engine", engine });
  }

</script>

{#each rows as r (r.row === "fold" ? `f${r.key}` : r.row === "images" ? `g${r.key}` : r.row === "word" ? `w${r.key}` : r.row === "stepline" ? `l${r.key}` : r.row === "steps" ? `s${r.key}` : `i${r.i}`)}
  {@const inner = r.row === "item" && !!r.inner}
  {#if r.row === "images"}
    <ImageCarousel images={r.images} {threadId} />
  {:else if r.row === "word"}
    <!-- The quietest level: a word that says it works, new with every step, and nothing else. -->
    <div class="activity now word" role="status" aria-live="off">
      <WorkGrid shape={workShape(`${threadId}:${r.key}`, prefs.workIndicator)} />
      <span class="text">{r.word}…</span>
    </div>
  {:else if r.row === "stepline"}
    {@const line = stepsLine(r.plan)}
    <!-- Minimal with a plan: one line, the step now, with the thread's one moving grid. -->
    <div class="activity now word" role="status" aria-live="off">
      <WorkGrid shape={workShape(`${threadId}:${r.key}`, prefs.workIndicator)} />
      <span class="of">{line.of}</span><span class="text">{line.text}</span>
    </div>
  {:else if r.row === "steps"}
    <Steps plan={r.plan} {threadId} live />
  {:else if r.row === "fold"}
    <div class="fold-row">
      <button class="line fold" class:running={r.running} onclick={() => (unfolded[r.key] = !r.open)} aria-expanded={r.open} data-tip={r.open ? "Hide the work" : "Show the work: the notes and every step"}>
        <!-- No pulse while it runs: the live step below pulses, and the status line keeps the clock. -->
        <span class="dot done" class:failed={r.failed}></span>
        <span class="text">{r.label}</span>
        <span class="chev" class:open={r.open}><Icon name="chevron" size={12} /></span>
      </button>
    </div>
  {:else}
  {@const item = r.item}
  {@const i = r.i}
  {#if item.kind === "user" && item.checkpoint}
    <Checkpoint {threadId} to={item.checkpoint} ts={item.ts} busy={live} />
  {/if}
  {#if item.kind === "user" && item.fromHub === "watch"}
    <!-- Hub woke the agent about the pull request it watches; what it said stays folded. -->
    <details class="note restored watch">
      <summary><Icon name="branch" size={13} />Pull request #{item.watch} changed at {clock(item.ts)}; Hub told the agent.</summary>
      <div class="watch-text">{item.text}</div>
    </details>
  {:else if item.kind === "user" && item.fromHub === "routine"}
    <!-- A routine started this run; what Hub asked stays folded. -->
    <details class="note restored watch">
      <summary><Icon name="clock" size={13} />The routine {item.routine} started this at {clock(item.ts)}. It only reports; answer to have something fixed.</summary>
      <div class="watch-text">{item.text}</div>
    </details>
  {:else if item.kind === "user" && item.fromHub === "widget"}
    <!-- The user asked for an Overview widget, or a change to one; Hub's note stays folded. -->
    <div class="user"><div class="bubble">{item.widget?.ask}</div></div>
    <details class="note restored watch">
      <summary><Icon name="spark" size={13} />{item.widget?.change ? "A change to this Overview widget" : "Asked for on the Overview as a widget"} at {clock(item.ts)}; Hub told the agent how widgets work.</summary>
      <div class="watch-text">{item.text}</div>
    </details>
  {:else if item.kind === "user" && item.fromHub === "vakten"}
    <!-- Hub handed Guard what is new on the Problems page; the list stays folded. -->
    <details class="note restored watch">
      <summary><Icon name="alert" size={13} />Hub handed Guard new problems at {clock(item.ts)}.</summary>
      <div class="watch-text">{item.text}</div>
    </details>
  {:else if item.kind === "user" && item.fromHub === "recipe"}
    <!-- A recipe run: Hub's note and the recipe's prompt stay folded. -->
    <details class="note restored watch">
      <summary><Icon name="play" size={13} />{item.recipe?.by ? `${item.recipe.by} ran` : "You ran"} the recipe {item.recipe?.name} here at {clock(item.ts)}.</summary>
      <div class="watch-text">{item.text}</div>
    </details>
  {:else if item.kind === "user" && (item.review || item.designReview || item.fromHub)}
    <!-- Hub sent this itself: a second opinion's findings, the leak guard's, or a request for
         proof. The line above says so. -->
  {:else if item.kind === "user"}
    <UserMessage {item} {threadId} {cwd} {live} />
  {:else if item.kind === "rewound"}
    <RewoundNote {item} />
  {:else if item.kind === "assistant" && webTurn.has(i)}
    {@const streaming = live && i === shown.length - 1}
    {@const answer = webAnswer(item.text)}
    <div class="assistant" class:inner use:menu={() => messageMenu(item.text)}>
      <StreamingText rich text={streaming ? item.text : answer.body} {streaming} />
      {#if !streaming && answer.links.length}
        <div class="sources">
          {#each answer.links.slice(0, 8) as l (l.url)}
            <button class="source" data-tip={l.url} onclick={() => openUrl(l.url).catch((e) => hub.fail(e))}>
              <span class="site" style:background={siteTint(l.host)}>{l.host[0]?.toUpperCase()}</span>
              <span class="what"><b>{l.title ?? l.host}</b>{#if l.title}<small>{l.host}</small>{/if}</span>
            </button>
          {/each}
        </div>
      {/if}
    </div>
  {:else if item.kind === "assistant"}
    <div class="assistant" class:inner use:menu={() => messageMenu(item.text)}><StreamingText rich text={item.text} streaming={live && i === shown.length - 1} /></div>
  {:else if item.kind === "child"}
    <ChildLine {item} parent={threadId} />
  {:else if item.kind === "task"}
    <TaskLine {item} parent={threadId} />
  {:else if item.kind === "plugin"}
    <PluginChatCard {item} />
  {:else if item.kind === "ask"}
    <AskLine {item} {threadId} />
  {:else if item.kind === "handoff_offer"}
    <HandoffOfferCard {item} {threadId} />
  {:else if item.kind === "page"}
    <PageCard {threadId} page={item} />
  {:else if item.kind === "activity" && item.web}
    <WebRow {item} last={i === shown.length - 1} {live} {tick} {inner} opened={(open[i] ?? detail === "all") || found(item)} ontoggle={() => (open[i] = !((open[i] ?? detail === "all") || found(item)))} />
  {:else if item.kind === "activity"}
    {@const opened = (open[i] ?? detail === "all") || found(item)}
    <ActivityRow {item} {threadId} now={live && i === shown.length - 1} {inner} {opened} ontoggle={() => (open[i] = !opened)} {detail} {tick} {stepsUp} {needle} />
  {:else if item.kind === "plan"}
    <Steps plan={item} {threadId} />
  {:else if item.kind === "permission"}
    <Permission {item} {threadId} />
  {:else if item.kind === "turn"}
    {@const reply = replyAt(i)}
    <Changes
      files={detail === "minimal" ? [] : (reply.files?.files ?? [])}
      proof={reply.files?.proof}
      docs={reply.files?.docs}
      answer={reply.answer}
      ts={item.ts}
      seq={item.seq}
      {cwd}
      {threadId}
      review={reply.files ? (items.findLast((x) => x.kind === "review" && x.state !== "failed" && x.seq >= item.seq) as ReviewItem | undefined) : undefined}
    />
  {:else if item.kind === "review"}
    <ReviewCard {item} {threadId} />
  {:else if item.kind === "design" || item.kind === "design_review" || item.kind === "design_strayed"}
    <DesignCard {item} {threadId} />
  {:else if item.kind === "proof"}
    <div class="note"><ProofStamp proof={item} />{#if item.docs}<DocsStamp docs={item.docs} />{/if}</div>
  {:else if item.kind === "desk"}
    <DeskNote {item} />
  {:else if item.kind === "clash"}
    <div class="note clash">
      <Icon name="branch" size={13} />
      <span>
        <button class="link inline" onclick={() => hub.open(item.other)}>{item.otherTitle}</button> is also changing
        <span class="mono">{item.paths.map(rel).join(", ")}</span>. Hub tells the agent with your next message.
      </span>
    </div>
  {:else if item.kind === "aim"}
    <div class="note aim" title="Read by {item.by}"><Icon name="plan" size={13} /><span>Works towards <b>{item.text}</b> in {item.stage}</span></div>
  {:else if item.kind === "topic"}
    <TopicNote text={item.text} seq={item.userSeq} by={item.by} {threadId} {cwd} />
  {:else if item.kind === "focus"}
    <FocusNote stage={item.stage} why={item.why} seq={item.seq} {threadId} {cwd} />
  {:else if item.kind === "leak"}
    <div class="note leak">
      <Icon name="shield" size={13} />
      <div>
        <div>Looks like a secret in what it wrote{item.sent ? "; sent to the agent to move out of the code" : ""}:</div>
        {#each item.findings as f, j (j)}
          <div class="finding"><span class="mono">{rel(f.path)}:{f.line}</span> {f.kind} <span class="mono dim">{f.hint}</span></div>
        {/each}
      </div>
    </div>
  {:else if item.kind === "customs" && item.declared}
    <CustomsAsk found={item.found} {threadId} />
  {:else if item.kind === "customs"}
    {@const stopped = item.found.filter((f) => f.status !== "cleared")}
    {@const stamped = item.found.filter((f) => f.status === "cleared")}
    {#if stamped.length}
      <div class="note restored">
        <Icon name="stamp" size={13} />
        <span>Cleared at Customs and stamped for every agent:
          {#each stamped as f, j (j)}<span class="mono" data-tip="{f.kind} in {rel(f.path)}">{f.name}</span>{j < stamped.length - 1 ? ", " : ""}{/each}</span>
        <button class="link" onclick={() => hub.openSettings("customs")}>Customs</button>
      </div>
    {/if}
    {#if stopped.length}
      <div class="note customs">
        <Icon name="stamp" size={13} />
        <div>
          <div>Not cleared at Customs yet{item.sent ? "; the agent was told" : ""}.
            <button class="link" onclick={() => hub.openSettings("customs")}>Customs</button></div>
          {#each stopped as f, j (j)}
            <div class="finding"><span class="mono">{f.name}</span> {f.kind} in <span class="mono dim">{rel(f.path)}</span>{f.status === "refused" ? ", refused" : ", held for you"}</div>
          {/each}
        </div>
      </div>
    {/if}
  {:else if item.kind === "attempt"}
    <div class="note restored">
      <Icon name="branch" size={13} />
      {#if item.state === "picked"}
        <span>Picked this attempt: {item.files} {item.files === 1 ? "file" : "files"} brought into <span class="mono" data-tip={item.cwd}>{folderName(item.cwd ?? "")}</span>, uncommitted. The others are archived.</span>
        {#if !live && item.safety}
          <button class="link" onclick={() => askUndo({ thread: threadId, to: item.safety!, label: "picking this attempt" })}>Undo</button>
        {/if}
      {:else}
        <span>Another attempt was picked{#if item.by}: <button class="link inline" onclick={() => hub.open(item.by!)}>open it</button>{/if}. This one's work is kept by Hub but not in the folder.</span>
      {/if}
    </div>
  {:else if item.kind === "terminal"}
    <div class="note restored">
      <Icon name="terminal" size={13} />
      {#if item.state === "taken"}Taken over in a terminal at {clock(item.ts)}.{:else}Back in Hub{item.turns ? `, with ${item.turns} ${item.turns === 1 ? "message" : "messages"} from the terminal` : ""}.{item.carryOn ? " The work was left unfinished, so Hub carries on." : ""}{/if}
    </div>
  {:else if item.kind === "wait"}
    {#if item.state === "switched"}
      {@const to = prov(item.to ?? "")}
      {@const from = item.from ? prov(item.from) : null}
      <Break kind="engine" note={item.switchedAt ? clock(item.switchedAt) : undefined}>
        {#if from}<ProviderMark id={from.id} size={14} />{from.name} ran out{:else}A plan limit ran out{/if}
        <span class="arrow">→</span> continued on <ProviderMark id={to.id} size={14} />{to.name}
      </Break>
    {:else if item.state === "waiting"}
      {@const out = ranOutToo(item)}
      {#if out}
        {@const p = prov(out)}
        <Break kind="stuck" note="both subscriptions are out"><ProviderMark id={p.id} size={14} />{p.name} ran out too</Break>
      {:else}
        <Break kind="waiting" note={item.limit}><svg class="glyph" viewBox="0 0 24 24" aria-hidden="true"><path d={GLYPHS.clock} /></svg>Stopped by a plan limit</Break>
      {/if}
      <div class="hint" data-tip={item.message}>
        Hub carries on {when(item.at)}{item.known ? "" : ", a guess since the engine did not say when it resets"}{out ? ", never with a paid API" : ""}
        {#if !live}<span>·</span><button class="link" onclick={() => api.quotaWait(threadId, "now")}>Carry on now</button><span>·</span><button class="link" onclick={() => api.quotaWait(threadId, "cancel")}>Don't</button>{/if}
      </div>
      {#if item.offer && hub.guard?.limit_switch === "ask" && !live && !waited[item.seq]}
        {@const p = prov(item.offer)}
        <div class="ask">
          <div>Keep going now on {p.name} with the whole conversation?</div>
          <div class="choices">
            <button class="choice" onclick={() => api.quotaWait(threadId, "switch", item.offer).catch((e) => hub.fail(e))}><ProviderMark id={p.id} size={14} />Continue in {p.name}</button>
            <button class="choice" onclick={() => (waited[item.seq] = true)}>Wait for the reset</button>
          </div>
        </div>
      {/if}
    {:else if item.state === "resumed"}
      <Break kind="waiting"><svg class="glyph" viewBox="0 0 24 24" aria-hidden="true"><path d={GLYPHS.clock} /></svg>{item.by === "you" ? "You carried on" : "The limit reset and Hub carried on"}</Break>
    {:else if item.state === "cancelled"}
      <Break kind="time">{item.by === "message" ? "You wrote before the limit reset, so Hub did not carry on" : "Not carrying on after the limit"}</Break>
    {:else}
      <Break kind="stuck" note="write to continue">Stopped by the limit again after {item.tries} {item.tries === 1 ? "try" : "tries"}</Break>
    {/if}
  {:else if item.kind === "sign_in"}
    <!-- The event names the machine as it calls itself (its hostname); a thread relayed from
         another machine opens the terminal by the name this one knows it under. -->
    {@const machine = host || item.machine}
    <div class="note error sign-in" data-tip={item.message}>
      <Icon name="key" size={13} />
      <span>
        {#if item.keychain}
          {item.label} is not signed in for Hub on {machine || "this machine"}: a Mac keeps its own login in the Keychain, which Hub's background service cannot read. Sign in with Hub once (it lasts a year), then send it again.
        {:else}
          {item.label}'s sign-in on {machine || "this machine"} {item.kept ? "that Hub kept stopped working" : "ran out"}, so this message did not go through.
          {#if item.canKeep}Sign in there again, then send it again.{:else}Sign in there with its own CLI, then send it again.{/if}
        {/if}
      </span>
      {#if machine && !live}
        <button class="link" onclick={() => signInOn(machine, item.engine, item.canKeep)}>Sign in on {machine}</button>
      {/if}
    </div>
  {:else if item.kind === "paused"}
    <div class="note restored">
      <Icon name="pause" size={13} />
      <span>Paused with every agent at {clock(item.ts)}{item.by && item.by !== hub.localHost ? ` from ${item.by}` : ""}. It carries on from here when you resume.</span>
    </div>
  {:else if item.kind === "restart" && item.by === "pause"}
    <div class="note restored">
      <Icon name="play" size={13} />
      <span>Resumed at {clock(item.ts)}. Asked the agent to carry on{item.queued ? `, then sent the ${item.queued === 1 ? "message" : `${item.queued} messages`} written meanwhile` : ""}.</span>
    </div>
  {:else if item.kind === "restart"}
    <div class="note restored">
      <Icon name="restart" size={13} />
      <span>
        {RESTARTED[item.by] ?? RESTARTED.crash} at {clock(item.ts)}, in the middle of this turn.
        {#if item.state === "continued"}Asked the agent to carry on{item.queued ? `, then sent the ${item.queued === 1 ? "message" : `${item.queued} messages`} written meanwhile` : ""}.
        {:else if item.why === "loop"}Not carried on: it was cut off several times in a row. Write to go on.
        {:else}Not carried on (turned off). Write to go on.{/if}
      </span>
    </div>
  {:else if item.kind === "moved"}
    <Break kind="machine" note={`${item.dirty ? `with ${item.dirty === 1 ? "1 uncommitted file" : `${item.dirty} uncommitted files`} · ` : ""}${clock(item.ts)}`}>
      <svg class="glyph" viewBox="0 0 24 24" aria-hidden="true"><path d={GLYPHS[machineGlyph(item.from, false)]} /></svg>{item.from}
      <span class="arrow">→</span>
      <svg class="glyph" viewBox="0 0 24 24" aria-hidden="true"><path d={GLYPHS[machineGlyph(thisMachine, !host)]} /></svg>{thisMachine}
    </Break>
    <div class="hint">The code came along, in <span class="mono">{item.folder}</span>. {item.stopped ? "The turn carries on." : "A fresh session starts with your next message."}</div>
  {:else if item.kind === "moved_away"}
    <Break kind="machine" note={clock(item.ts)}>
      <svg class="glyph" viewBox="0 0 24 24" aria-hidden="true"><path d={GLYPHS[machineGlyph(thisMachine, !host)]} /></svg>{thisMachine}
      <span class="arrow">→</span>
      <svg class="glyph" viewBox="0 0 24 24" aria-hidden="true"><path d={GLYPHS[machineGlyph(item.to, false)]} /></svg>{item.to}
    </Break>
    <div class="hint">Open it there.</div>
  {:else if item.kind === "switch"}
    {@const p = prov(item.to)}
    {@const f = item.from ? prov(item.from) : null}
    <Break kind="engine" note={clock(item.ts)}>
      {#if f}<ProviderMark id={f.id} size={14} />{f.name} <span class="arrow">→</span>{/if}
      switched to <ProviderMark id={p.id} size={14} />{p.name}
    </Break>
    <div class="hint"><HandoverHint {threadId} ts={item.ts} carried={item.carried} ticks={items.length} /></div>
  {:else if item.kind === "new"}
    <!-- What happened since you last looked: a break, with how much and since when. -->
    {@const n = items.slice(items.indexOf(item) + 1).filter((i) => i.kind === "assistant").length}
    <div class="new-mark" data-new-mark>
      <Break kind="new" note={[n ? `${n} ${n === 1 ? "reply" : "replies"}` : "", ago(item.since)].filter(Boolean).join(" · ")}>New</Break>
    </div>
  {:else if item.kind === "idle"}
    <Break kind="time" note={later(item.gap)}>New</Break>
  {:else if item.kind === "handed_off"}
    <!-- Handed to one of your tools ("Hand to Knegarn"); what the tool printed stays folded. -->
    <details class="note restored watch" class:error={!item.ok}>
      <summary><Icon name="share" size={13} />{item.ok ? `Handed to ${item.tool} at ${clock(item.ts)}` : `${item.tool} refused it at ${clock(item.ts)}`}: {item.title}</summary>
      {#if item.output}<div class="watch-text mono">{item.output}</div>{/if}
    </details>
  {:else if item.kind === "worker_run"}
    <WorkerRunCard run={item.run} lines={item.lines} ts={item.ts} {threadId} />
  {:else if item.kind === "handoff_result"}
    <!-- The handed-off work ended; what the tool said stays folded. -->
    <details class="note restored watch" class:error={item.state !== "done" || item.proven === false}>
      <summary>
        <Icon name="share" size={13} />{item.tool}
        {item.state === "done" ? (item.proven ? "finished it, verified" : item.proven === false ? "finished it, verification failed" : "finished it") : "stopped"}
        at {clock(item.ts)}: {item.title}{item.cost != null ? ` · ≈ $${item.cost.toFixed(2)}` : ""}
        {#if item.link}{@const link = item.link}<button class="link inline" data-tip={link} onclick={(e) => { e.preventDefault(); openUrl(link).catch((err) => hub.fail(err)); }}>{link.includes("/pull/") ? "Pull request" : "Open"}</button>{/if}
      </summary>
      {#if item.said}<div class="watch-text">{item.said}</div>{/if}
    </details>
  {:else if item.kind === "quota"}
    <div class="note restored"><Icon name="gauge" size={13} />Switched to {item.to}: {item.limit.toLowerCase()} at {Math.round(item.percent)}%.</div>
  {:else if item.kind === "auto"}
    <!-- Every move Auto makes is shown; starting on Auto is not a move. -->
    {#if "on" in item}
      {#if item.by !== "start"}
        <div class="note restored"><Icon name="auto" size={13} />{item.on ? "Auto on." : "Auto off."}</div>
      {/if}
    {:else if item.to}
      <div class="note restored"><Icon name="auto" size={13} />Auto: {item.ask === "quick" ? `${item.toName} for this quick question` : `back to ${item.toName} for the work`}.</div>
    {:else}
      {@const p = prov(item.engine)}
      <div class="note restored switch"><Icon name="auto" size={13} />Auto started this on <ProviderMark id={p.id} size={13} />{p.name}: {item.why}.</div>
    {/if}
  {:else if item.kind === "restored"}
    <div class="note restored">
      <Icon name="undo" size={13} />
      <span>
        {item.seq != null ? `Rolled back to before “${short(item.text ?? "")}”` : "Rollback undone"}:
        {item.files} {item.files === 1 ? "file" : "files"}{#if item.commits}, {item.commits} {item.commits === 1 ? "commit" : "commits"}{/if}{#if item.keptCommits}; commits kept because {item.keptCommits}{/if}.
      </span>
      {#if !live}
        <button class="link" onclick={() => askUndo({ thread: threadId, to: item.safety, label: "this rollback" })}>Undo</button>
      {/if}
    </div>
    <Checkpoint {threadId} to={item.safety} busy={live} hidden />
  {:else if item.kind === "notice"}
    <!-- What the engine said beside its reply, quiet; colour only for an error. -->
    <div class="note restored notice" class:error={item.severity === "error"}>
      <Icon name="alert" size={13} />
      <span>{item.title}{#if item.description}<span class="dim"> {item.description}</span>{/if}{#if /skills? context budget/i.test(item.title)}
        <button class="link inline" onclick={() => hub.openSettings("extensions")}>Choose which plugins Hub's threads load</button>{/if}</span>
    </div>
  {:else if item.kind === "refused"}
    <div class="note"><Icon name="shield" size={13} /><span>Hub refused <span class="mono">{item.title}</span>: {item.why}.</span></div>
  {:else if item.kind === "end"}
    <div class="note">Stopped ({item.reason.replace("_", " ")})</div>
  {:else if item.kind === "error"}
    <ErrorCard {item} {threadId} {host} {live} retry={retryOf(item)} />
  {/if}
  {/if}
{/each}

<EmojiPicker {threadId} />

<style>
  /* @kit transcript */
</style>
