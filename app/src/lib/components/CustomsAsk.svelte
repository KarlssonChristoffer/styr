<!-- Something the agent declared at Customs and is waiting for. Cleared here, the agent hears so and carries on, without a trip to Settings and a message typed by hand.
     Used by Transcript.svelte. -->
<script lang="ts">
  import { quietly } from "../quiet";
  // Something the agent declared at Customs and is waiting for. Cleared here, the agent hears so
  // and carries on, without a trip to Settings and a message typed by hand.
  import { api } from "../api";
  import type { CustomsItem } from "../bindings/CustomsItem";
  import { hub } from "../state.svelte";
  import type { CustomsFinding } from "../transcript";
  import Icon from "./Icon.svelte";

  let { found, threadId }: { found: CustomsFinding[]; threadId: string } = $props();

  // Where each stands on Customs' list now, which may have changed since it was declared.
  let now = $state<Record<string, string>>({});
  const key = (f: { kind: string; name: string }) => `${f.kind}:${f.name}`;
  $effect(() => {
    void found;
    api
      .customs()
      .then((list: CustomsItem[]) => (now = Object.fromEntries(list.map((m) => [key(m), m.status]))))
      .catch(quietly("customs status"));
  });

  let busy = $state(false);
  async function decide(f: CustomsFinding, status: "cleared" | "refused") {
    busy = true;
    try {
      const m = await api.editCustoms({ kind: f.kind, name: f.name, status });
      now = { ...now, [key(f)]: m.status };
      const text =
        status === "cleared"
          ? `${f.name} (${f.kind}) is cleared at Customs. Go ahead with it.`
          : `${f.name} (${f.kind}) is refused at Customs. Do it without it.`;
      await hub.send(threadId, text);
    } catch (e) {
      hub.fail(e);
    } finally {
      busy = false;
    }
  }
</script>

{#each found as f (key(f))}
  {@const status = now[key(f)] ?? f.status}
  <div class="ask" class:held={status === "held"}>
    <Icon name="stamp" size={13} />
    <div class="body">
      <div>
        {#if status === "held"}Held at Customs:{:else if status === "cleared"}Cleared at Customs:{:else}Refused at Customs:{/if}
        <span class="mono">{f.name}</span> <span class="dim">{f.kind}</span>
      </div>
      {#if f.why}<div class="why">{f.why}</div>{/if}
      {#if status === "held"}
        <div class="actions">
          <button class="btn primary" disabled={busy} onclick={() => decide(f, "cleared")}>Clear and carry on</button>
          <button class="btn" disabled={busy} onclick={() => decide(f, "refused")}>Refuse</button>
          <button class="link" onclick={() => hub.openSettings("customs")}>Customs</button>
        </div>
      {/if}
    </div>
  </div>
{/each}

<style>
  .ask { display: flex; align-items: flex-start; gap: 8px; color: var(--text-3); background: var(--surface); border-radius: var(--r-md); padding: 8px 12px; margin: 6px 0; }
  .ask.held { color: var(--amber); background: var(--amber-bg); }
  .ask :global(svg) { margin-top: 2px; flex: none; }
  .body { display: flex; flex-direction: column; gap: 4px; min-width: 0; }
  .why { color: var(--text-3); }
  .mono { font-family: var(--mono); font-size: 12px; color: var(--text); }
  .dim { color: var(--muted); }
  .actions { display: flex; align-items: center; gap: 6px; margin-top: 4px; }
  .link { color: var(--muted); text-decoration: underline; margin-left: 4px; }
</style>
