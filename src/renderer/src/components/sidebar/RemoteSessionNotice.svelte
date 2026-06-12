<script lang="ts">
  import { Smartphone, Wifi } from '@lucide/svelte'
  import type { RemoteSessionStatus } from '../../../../main/remote/types'

  let {
    status,
    showQrAdapter,
    hasQrInterface,
  }: {
    status: RemoteSessionStatus
    showQrAdapter: boolean
    hasQrInterface: boolean
  } = $props()
</script>

{#if status.kind === 'paired'}
  <div
    class="flex items-center gap-2 text-xs text-text-secondary bg-bg-input border border-border-subtle rounded-md px-2.5 py-2"
  >
    <Smartphone size={13} class="shrink-0 text-success" />
    <span class="truncate">Remote device is controlling this window.</span>
  </div>
{:else if status.kind === 'peerArrived'}
  <div
    class="text-xs text-warning-text bg-bg-input border border-border-subtle rounded-md px-2.5 py-2"
  >
    Accept or reject the device request in the approval dialog.
  </div>
{:else if status.kind === 'listening'}
  <div
    class="flex items-center gap-2 text-xs text-text-secondary bg-bg-input border border-border-subtle rounded-md px-2.5 py-2"
  >
    <Wifi size={13} class="shrink-0 text-warning-text" />
    <span class="truncate">
      {status.lanIp === '0.0.0.0'
        ? 'Trusted devices may reconnect on any adapter.'
        : 'Trusted devices may reconnect.'}
    </span>
  </div>
{:else if showQrAdapter && !hasQrInterface}
  <div
    class="text-xs text-warning-text bg-bg-input border border-border-subtle rounded-md px-2.5 py-2"
  >
    Select an adapter before pairing.
  </div>
{/if}
