<script lang="ts">
  import { Play, Square, Wifi } from '@lucide/svelte'
  import type { RemoteSessionStatus } from '../../../../main/remote/types'
  import CustomSelect from '../shared/CustomSelect.svelte'

  type SelectOption = { value: string; label: string }
  type SelectGroup = { label: string; options: SelectOption[] }

  let {
    status,
    busy,
    canListen,
    hasSelectedInterface,
    selectedInterface,
    interfaceGroups,
    listenerScope,
    onStartListening,
    onStartPairing,
    onStopSession,
    onSetInterface,
    onSetListenerScope,
  }: {
    status: RemoteSessionStatus
    busy: boolean
    canListen: boolean
    hasSelectedInterface: boolean
    selectedInterface: string
    interfaceGroups: SelectGroup[]
    listenerScope: string
    onStartListening: () => void
    onStartPairing: () => void
    onStopSession: () => void
    onSetInterface: (name: string) => void
    onSetListenerScope: (value: string) => void
  } = $props()
</script>

{#if status.kind === 'idle' || status.kind === 'error'}
  <div class="grid grid-cols-2 gap-1">
    <button
      type="button"
      class="inline-flex items-center justify-center gap-1 h-7 rounded-md border-0 bg-active text-text text-xs font-medium cursor-pointer enabled:hover:bg-hover disabled:opacity-50 disabled:cursor-wait focus-visible:outline-2 focus-visible:outline-focus-ring focus-visible:outline-offset-1"
      disabled={busy || !canListen}
      onclick={onStartListening}
    >
      <Wifi size={13} />
      Listen
    </button>
    <button
      type="button"
      class="inline-flex items-center justify-center gap-1 h-7 rounded-md border-0 bg-accent-bg text-accent-text text-xs font-medium cursor-pointer enabled:hover:bg-accent-bg-hover disabled:opacity-50 disabled:cursor-wait focus-visible:outline-2 focus-visible:outline-focus-ring focus-visible:outline-offset-1"
      disabled={busy || !hasSelectedInterface}
      onclick={onStartPairing}
    >
      <Play size={13} />
      Pair
    </button>
  </div>
{:else if status.kind === 'listening'}
  <div class="grid grid-cols-2 gap-1">
    <button
      type="button"
      class="inline-flex items-center justify-center gap-1 h-7 rounded-md border-0 bg-accent-bg text-accent-text text-xs font-medium cursor-pointer enabled:hover:bg-accent-bg-hover disabled:opacity-50 disabled:cursor-wait focus-visible:outline-2 focus-visible:outline-focus-ring focus-visible:outline-offset-1"
      disabled={busy || !hasSelectedInterface}
      onclick={onStartPairing}
    >
      <Play size={13} />
      Pair
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

<div class="flex flex-col gap-2 pt-2 border-t border-border-subtle">
  <label class="flex flex-col gap-1 text-2xs uppercase tracking-caps-tight text-text-faint">
    Adapter
    <CustomSelect value={selectedInterface} groups={interfaceGroups} onchange={onSetInterface} />
  </label>
  <label class="flex flex-col gap-1 text-2xs uppercase tracking-caps-tight text-text-faint">
    Listener
    <CustomSelect
      value={listenerScope}
      onchange={onSetListenerScope}
      options={[
        { value: 'selected', label: 'Selected adapter' },
        { value: 'all', label: 'All adapters' },
      ]}
    />
  </label>
</div>
