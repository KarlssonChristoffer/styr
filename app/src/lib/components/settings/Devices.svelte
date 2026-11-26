<!-- Settings page: access. Used by Settings.svelte. -->
<script lang="ts">
  import Quiet from "../Quiet.svelte";
  import { quiet } from "../../quiet";
  import { api } from "../../api";
  import type { Device } from "../../bindings/Device";
  import type { PairCode } from "../../bindings/PairCode";
  import type { PhoneLink } from "../../bindings/PhoneLink";
  import { ago } from "../../format";
  import { hub } from "../../state.svelte";
  import AgentKeys from "../AgentKeys.svelte";

  // Devices let into this machine with a pairing code, and a fresh code for the next one.
  let devices = $state<Device[] | null>(null);

  let code = $state<PairCode | null>(null);

  async function newPairCode() {
    try {
      code = await api.pairCode();
    } catch (e) {
      hub.fail(e);
    }
  }

  // The phone's QR code, made when asked for.
  let phone = $state<PhoneLink | null>(null);

  let phoneBusy = $state(false);

  async function connectPhone() {
    phoneBusy = true;
    try {
      phone = await api.phoneLink();
    } catch (e) {
      hub.fail(e);
    } finally {
      phoneBusy = false;
    }
  }

  async function revoke(id: string) {
    try {
      await api.revokeDevice(id);
      devices = (devices ?? []).filter((d) => d.id !== id);
    } catch (e) {
      hub.fail(e);
    }
  }

  $effect(() => {
    if (hub.connected) quiet(api.devices().then((d) => (devices = d)), "devices");
  });
</script>

<h1>Access</h1>
<section>
  <div class="line">
    <div class="what">
      <div class="name">Your phone</div>
      <div class="hint">
        {#if phone?.link}
          Scan it with the phone's camera. It works once, until {new Date(phone.expires_at).toLocaleTimeString([], { hour: "2-digit", minute: "2-digit" })}, and only on your tailnet.
        {:else if phone?.problem}
          <span class="problem">No address on your tailnet: {phone.problem}.</span> The phone and this machine need Tailscale, signed in to the same tailnet.
        {:else}
          Follow threads, write to them and answer what they ask, from your phone over your tailnet. The phone may do nothing else.
        {/if}
      </div>
    </div>
    <div class="control">
      <button class="btn" disabled={phoneBusy} onclick={connectPhone}>{phoneBusy ? "Starting…" : phone ? "New code" : "Connect your phone"}</button>
    </div>
  </div>
  {#if phone?.qr && phone.link}
    <div class="phone">
      <!-- The core draws the code itself (`phone.rs`); nothing from outside is in it. -->
      <div class="qr">{@html phone.qr}</div>
      <div class="mono link">{phone.link.replace(/#.*/, "")}</div>
    </div>
  {/if}
</section>
<h2>Devices</h2>
<section class="list">
  {#each devices ?? [] as d (d.id)}
    <div class="change">
      <span class="subject">{d.name}</span>
      {#if d.phone}<span class="when" data-tip="May follow and answer threads, from the tailnet">phone</span>{/if}
      <span class="when" data-tip="Paired">{ago(d.created_at)}</span>
      {#if d.last_seen}<span class="when" data-tip="Last call">seen {ago(d.last_seen)}</span>{/if}
      <button class="btn small" data-tip="Its token stops working at once" onclick={() => revoke(d.id)}>Revoke</button>
    </div>
  {:else}
    {#if devices}<Quiet text="No device has paired with this machine" />{:else}<Quiet kind="loading" />{/if}
  {/each}
</section>
<section>
  <div class="line">
    <div class="what">
      <div class="name">Let another device in</div>
      <div class="hint">
        {#if code}
          Code <span class="mono pair">{code.code}</span>, once, until {new Date(code.expires_at).toLocaleTimeString([], { hour: "2-digit", minute: "2-digit" })}{#if code.url}, at <span class="mono">{code.url}</span>{:else}. Tailscale does not serve this machine yet: <span class="mono">hubd pair --tailscale</span>{/if}.
        {:else}
          A one-time code for another machine's Hub. Each device gets a token of its own.
        {/if}
      </div>
    </div>
    <div class="control"><button class="btn" onclick={newPairCode}>{code ? "New code" : "Make a code"}</button></div>
  </div>
</section>

<AgentKeys />

<style>
  /* @kit settings */
</style>
