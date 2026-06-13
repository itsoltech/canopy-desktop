<script lang="ts">
  import { Play, Plus, Square, X } from '@lucide/svelte'
  import type { RemoteSessionStatus } from '../../../../main/remote/types'

  let {
    status,
    busy,
    canListen,
    pairingActive,
    onStartListening,
    onStartPairing,
    onCancelPairing,
    onStopSession,
  }: {
    status: RemoteSessionStatus
    busy: boolean
    canListen: boolean
    pairingActive: boolean
    onStartListening: () => void
    onStartPairing: () => void
    onCancelPairing: () => void
    onStopSession: () => void
  } = $props()
</script>

{#if pairingActive}
  <button
    type="button"
    class="inline-flex items-center justify-center gap-1 w-full h-7 rounded-md border-0 bg-danger-bg text-danger-text text-xs font-medium cursor-pointer enabled:hover:bg-hover disabled:opacity-50 disabled:cursor-wait focus-visible:outline-2 focus-visible:outline-focus-ring focus-visible:outline-offset-1"
    disabled={busy}
    onclick={onCancelPairing}
  >
    <X size={13} />
    Cancel
  </button>
{:else if status.kind === 'idle' || status.kind === 'error'}
  <button
    type="button"
    class="inline-flex items-center justify-center gap-1 w-full h-7 rounded-md border-0 bg-success-bg text-success-text text-xs font-medium cursor-pointer enabled:hover:bg-[color-mix(in_srgb,var(--color-success)_30%,transparent)] disabled:opacity-50 disabled:cursor-wait focus-visible:outline-2 focus-visible:outline-focus-ring focus-visible:outline-offset-1"
    disabled={busy || !canListen}
    onclick={onStartListening}
  >
    <Play size={13} />
    Start listening
  </button>
{:else if status.kind === 'listening'}
  <div class="grid grid-cols-2 gap-1">
    <button
      type="button"
      class="inline-flex items-center justify-center gap-1 h-7 rounded-md border-0 bg-accent-bg text-accent-text text-xs font-medium cursor-pointer enabled:hover:bg-accent-bg-hover disabled:opacity-50 disabled:cursor-wait focus-visible:outline-2 focus-visible:outline-focus-ring focus-visible:outline-offset-1"
      disabled={busy}
      onclick={onStartPairing}
    >
      <Plus size={13} />
      Pair device
    </button>
    <button
      type="button"
      class="inline-flex items-center justify-center gap-1 h-7 rounded-md border-0 bg-danger-bg text-danger-text text-xs font-medium cursor-pointer enabled:hover:bg-hover disabled:opacity-50 disabled:cursor-wait focus-visible:outline-2 focus-visible:outline-focus-ring focus-visible:outline-offset-1"
      disabled={busy}
      onclick={onStopSession}
    >
      <Square size={12} />
      Stop
    </button>
  </div>
{:else}
  <button
    type="button"
    class="inline-flex items-center justify-center gap-1 w-full h-7 rounded-md border-0 bg-danger-bg text-danger-text text-xs font-medium cursor-pointer enabled:hover:bg-hover disabled:opacity-50 disabled:cursor-wait focus-visible:outline-2 focus-visible:outline-focus-ring focus-visible:outline-offset-1"
    disabled={busy}
    onclick={onStopSession}
  >
    <Square size={12} />
    Stop
  </button>
{/if}
