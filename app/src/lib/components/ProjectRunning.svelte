<!-- För: agent. What runs in a project's folder, and what it uses that it did not start (ProjectView). -->
<script lang="ts">
  import { hub } from "../state.svelte";
  import { folderName } from "../format";
  import { menu } from "../menu.svelte";
  import { serverMenu } from "../menus.svelte";
  import type { ProjectRuntime } from "../bindings/ProjectRuntime";
  import Quiet from "./Quiet.svelte";

  let {
    kind,
    runtime,
    full = false,
    stopping = null,
    onstop,
    threadTitle,
    projectOf,
  }: {
    kind: "running" | "uses";
    runtime: ProjectRuntime | null;
    /** Show the command and ports of each too (the Services tab). */
    full?: boolean;
    stopping?: number | null;
    onstop: (pid: number) => void;
    threadTitle: (id: string | null) => string | null;
    projectOf: (cwd: string | null) => { name: string; cwd: string } | null;
  } = $props();
</script>

{#if kind === "running"}
  {#if runtime}
    {#each runtime.servers as srv (srv.pid)}
      <div class="proc" use:menu={() => serverMenu({ ...srv, port: srv.ports[0], process: srv.name })}>
        <span class="live"></span>
        <span class="ports mono">{srv.ports.map((p) => `:${p}`).join(" ")}</span>
        <span class="grow">
          <span class="t">{srv.name}</span>
          {#if threadTitle(srv.thread_id)}
            <button class="m link-thread" onclick={() => hub.open(srv.thread_id!)}> · from {threadTitle(srv.thread_id)}</button>
          {:else}<span class="m"> · {srv.service ? `service ${srv.service}` : srv.thread_id ? "from a thread" : "in the folder"}</span>{/if}
          {#if full}<span class="cmd mono" data-tip={srv.command}>{srv.command}</span>{/if}
        </span>
        <button class="btn danger" disabled={stopping === srv.pid} onclick={() => onstop(srv.pid)}>Stop</button>
      </div>
    {/each}
    {#each runtime.containers as c (c.id)}
      <div class="proc">
        <span class="live"></span>
        <span class="ports mono">docker</span>
        <span class="grow"><span class="t">{c.name}</span><span class="m"> · {c.image} · {c.status}</span>
          {#if full && c.ports}<span class="cmd mono">{c.ports}</span>{/if}
        </span>
      </div>
    {/each}
    {#if !runtime.servers.length && !runtime.containers.length}
      <Quiet text="Nothing running" />
    {/if}
    {#if runtime.other_processes}
      <p class="m other">+ {runtime.other_processes} other {runtime.other_processes === 1 ? "process" : "processes"} without a port</p>
    {/if}
  {:else}
    <Quiet kind="loading" />
  {/if}
{:else}
  {#each runtime?.uses ?? [] as u (u.port)}
    {@const owner = projectOf(u.cwd)}
    <div class="proc" use:menu={() => serverMenu(u)}>
      <span class="ports mono">:{u.port}</span>
      <span class="grow">
        <span class="t">{owner?.name ?? u.process ?? "unknown"}</span>
        {#if owner}<span class="m"> · {u.process}</span>{:else if u.cwd}<span class="m mono" data-tip={u.cwd}> · {folderName(u.cwd)}</span>{/if}
      </span>
      {#if owner}<button class="btn" onclick={() => hub.openProject(owner.cwd)}>Open</button>{/if}
    </div>
  {/each}
{/if}

<style>
  /* @kit projectView */
</style>
