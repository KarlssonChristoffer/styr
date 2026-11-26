<!-- För: agent. The groups of what runs on a machine: services, builds, containers and the rest (MachineView). -->
<script lang="ts">
  import { hub } from "../state.svelte";
  import { folderName } from "../format";
  import { GLYPHS } from "../refs";
  import { span } from "../sessions.svelte";
  import type { HostSnapshot } from "../bindings/HostSnapshot";
  import type { WorkerProcess } from "../bindings/WorkerProcess";
  import type { Workers } from "../bindings/Workers";
  import Icon from "./Icon.svelte";
  import MachineGroup from "./MachineGroup.svelte";
  import MachineRow from "./MachineRow.svelte";
  import Quiet from "./Quiet.svelte";
  import StackLogo from "./StackLogo.svelte";

  let {
    kind,
    name,
    snap,
    work,
    services,
    servicesMem,
    procs,
    roots,
    hubMem,
    memTotal,
    rest,
    stopPid,
  }: {
    kind: "services" | "builds" | "containers" | "else";
    name: string;
    snap: HostSnapshot | null;
    work: Workers | null;
    services: HostSnapshot["listeners"];
    servicesMem: number;
    procs: Map<number, WorkerProcess>;
    roots: { p: WorkerProcess; mem: number; cpu: number; more: number }[];
    hubMem: number;
    memTotal: number;
    rest: number;
    stopPid: (pid: number) => Promise<void>;
  } = $props();
</script>

{#if kind === "services"}
  {#if snap}
  <MachineGroup machine={name} id="services" title="Services and ports" count={services.length} mem={servicesMem} total={memTotal} tone="#78c4e6">
    {#snippet icon()}<svg class="gi" viewBox="0 0 24 24" aria-hidden="true"><path d={GLYPHS.port} /></svg>{/snippet}
    {#each services as l (`${l.port}-${l.pid}-${l.protocol}`)}
      {@const proc = l.pid != null ? procs.get(l.pid) : undefined}
      {@const who = l.thread_id ? hub.threads.find((t) => t.id === l.thread_id) : undefined}
      <MachineRow
        tag=":{l.port}"
        title={l.is_hub ? "Hub" : (l.process ?? "?")}
        sub={who ? `thread: ${who.title}` : (l.command ?? "")}
        mem={proc?.mem ?? 0}
        cpu={proc ? proc.cpu_percent : null}
        total={memTotal}
        time={l.started_at ? span(Date.now() - l.started_at) : ""}
        tone="#78c4e6"
        ok={!l.is_hub && l.origin === "thread" && (!who || who.archived || who.settled_at != null)}
        ask={!!l.thread_id}
        lock={l.is_hub}
        stop={l.is_hub || l.pid == null ? null : () => stopPid(l.pid!)}
      >
        {#snippet icon()}<svg class="gi" viewBox="0 0 24 24" aria-hidden="true"><path d={GLYPHS.port} /></svg>{/snippet}
      </MachineRow>
    {:else}<Quiet text="Nothing listening" />{/each}
  </MachineGroup>
  {/if}
{:else if kind === "builds"}
  {#if snap}
  <MachineGroup machine={name} id="builds" title="Builds and Hub" count={roots.length} mem={hubMem} total={memTotal} tone="#e0a96d">
    {#snippet icon()}<Icon name="workers" size={15} />{/snippet}
    {#each roots as r, i (r.p.pid)}
      <MachineRow
        title={i === 0 ? "Hub core" : r.p.name}
        sub={`${r.p.command}${r.more ? ` · +${r.more} more` : ""}`}
        mem={r.mem}
        cpu={r.cpu}
        total={memTotal}
        time={r.p.started_at ? span(Date.now() - r.p.started_at) : ""}
        tone="#e0a96d"
        lock
      >
        {#snippet icon()}<Icon name={i === 0 ? "spark" : "workers"} size={15} />{/snippet}
      </MachineRow>
    {:else}<Quiet kind={work ? "none" : "loading"} />{/each}
  </MachineGroup>
  {/if}
{:else if kind === "containers"}
  {#if work?.containers.length}
    <MachineGroup machine={name} id="containers" title="Containers" count={work.containers.length} tone="#2496ed">
      {#snippet icon()}<StackLogo id="docker" size={15} />{/snippet}
      {#each work.containers as c (c.id)}
        <MachineRow title={c.name} sub={[c.image, c.project ? folderName(c.project) : "", c.ports].filter(Boolean).join(" · ")} time={c.status.replace(/^Up /, "")} tone="#2496ed" lock>
          {#snippet icon()}<StackLogo id="docker" size={15} />{/snippet}
        </MachineRow>
      {/each}
    </MachineGroup>
  {/if}
{:else if kind === "else"}
  {#if work}
    <MachineGroup machine={name} id="else" title="Everything else" count={0} mem={rest} total={memTotal} open={false}>
      {#snippet icon()}<Icon name="more" size={15} />{/snippet}
      <MachineRow title="The rest of this machine" sub="System and other programs." mem={rest} total={memTotal} tone="var(--text-3)" lock>
        {#snippet icon()}<Icon name="more" size={15} />{/snippet}
      </MachineRow>
    </MachineGroup>
  {/if}
{/if}

<style>
  .gi { width: 15px; height: 15px; fill: none; stroke: currentColor; stroke-width: 1.75; stroke-linecap: round; stroke-linejoin: round; }
</style>
