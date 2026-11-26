<!-- För: agent. One line of the agent's work and the steps under it once opened (Transcript). -->
<script lang="ts">
  import { duration, failedStep, glyph, lastLine, live as liveLabel, running as isRunning, summary } from "../activity";
  import { hub } from "../state.svelte";
  import type { Activity } from "../activity";
  import { prefs } from "../prefs.svelte";
  import { workShape } from "../workGrid";
  import { hits } from "../transcriptWords";
  import Icon from "./Icon.svelte";
  import TerminalOutput from "./TerminalOutput.svelte";
  import WorkGrid from "./WorkGrid.svelte";

  let {
    item,
    threadId,
    now,
    inner = false,
    opened,
    ontoggle,
    detail,
    tick,
    stepsUp,
    needle,
  }: {
    item: Activity;
    threadId: string;
    /** It is the thread's live row. */
    now: boolean;
    inner?: boolean;
    opened: boolean;
    ontoggle: () => void;
    detail: string;
    /** The clock, ticking while the thread works. */
    tick: number;
    /** The steps card is up and holds the thread's one moving grid. */
    stepsUp: boolean;
    /** What Ctrl F looks for, lower case. */
    needle: string;
  } = $props();

  const toolStatus: Record<string, string> = {
    pending: "Waiting",
    in_progress: "Running",
    completed: "Done",
    failed: "Failed",
  };
  const last = $derived(item.steps.at(-1));
  const failed = $derived(item.steps.some((s) => s.kind === "tool" && s.status === "failed"));
  /** Opens the thread's terminal at a command. */
  const showTerminal = (id: string) => hub.showCommand(id);
</script>

<div class="activity" class:now class:inner>
  <button class="line" onclick={() => ontoggle()} aria-expanded={opened}>
    <!-- The status line below has the one moving grid; every row above it leads with the same quiet dot. --><span class="dot" class:failed></span>
    <span class="text">{now && last ? (isRunning(last) || last.kind === "thought" ? liveLabel(last) : `${summary(item.steps)}…`) : summary(item.steps)}</span>
    <span class="chev" class:open={opened}><Icon name="chevron" size={12} /></span>
  </button>
  {#if now && detail !== "quiet" && last?.kind === "tool" && last.terminal && isRunning(last)}
    <!-- The running command's last lines, so you can see it is getting on without opening anything. -->
    <button class="tail" onclick={() => showTerminal(last.id)} data-tip="Open the terminal (Ctrl §)">
      {#if last.output}
        <TerminalOutput text={last.output} max={4} earlier={false} />
      {:else}
        <span class="quiet">No output yet</span>
      {/if}
      <span class="took"><Icon name="expand" size={11} /></span>
    </button>
  {/if}
  {#if opened}
    <div class="steps">
      {#each item.steps as step, j (j)}
        {#if step.kind === "thought"}
          <details class="step" open={hits(needle, step.text) || undefined}>
            <summary><span class="glyph"><Icon name="brain" size={12} /></span><span class="name">Thinking</span></summary>
            <p>{step.text}</p>
          </details>
        {:else}
          {@const busy = isRunning(step)}
          {@const bad = failedStep(step)}
          {@const end = busy ? "" : lastLine(step.terminal ? step.output : bad ? step.detail : undefined)}
          <details class="step {step.status}" class:bad open={hits(needle, step.detail) || detail === "all" || undefined}>
            <summary>
              <span class="glyph"><Icon name={glyph(step)} size={12} /></span>
              <span class="name mono">{step.title}</span>
              <span class="status">
                {#if busy && step.status === "in_progress"}
                  {#if !stepsUp}<WorkGrid shape={workShape(`${threadId}:${step.id ?? step.title}`, prefs.workIndicator)} size={11} hold />{/if}{step.since ? duration(Math.max(0, tick - step.since)) : toolStatus.in_progress}
                {:else if busy}
                  {toolStatus.pending}
                {:else}
                  {#if step.since && step.until && step.until - step.since >= 1000}<span class="took">{duration(step.until - step.since)}</span>{/if}
                  {#if step.terminal && step.exit != null}
                    <span class="code" class:bad={step.exit !== 0} title="Exit code">exit {step.exit}</span>
                  {:else if step.status === "failed"}
                    <span class="code bad">Failed</span>
                  {/if}
                {/if}
              </span>
              {#if end}<span class="end mono" title={end}>{end}</span>{/if}
            </summary>
            {#if step.terminal}
              {#if step.command && !step.title.replace(/`/g, "").includes(step.command)}<div class="cmdline mono">$ {step.command}</div>{/if}
              {#if step.output}<TerminalOutput text={step.output} cut={step.cut} scroll />{/if}
              <div class="exit" class:failed={step.exit != null && step.exit !== 0}>
                {step.exit != null && step.exit !== 0 ? `Exited with ${step.exit}` : step.output || isRunning(step) ? "" : "No output"}
                <button class="link" onclick={() => showTerminal(step.id)}>Open in terminal</button>
              </div>
            {:else if step.detail}<pre>{step.detail}</pre>{/if}
          </details>
        {/if}
      {/each}
    </div>
  {/if}
</div>
<style>
  /* @kit transcript */
</style>
