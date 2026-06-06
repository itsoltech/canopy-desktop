<script lang="ts">
  import { onDestroy, onMount } from 'svelte'
  import { match } from 'ts-pattern'
  import { Check, Copy, Play, Settings, Smartphone, Square, Wifi } from '@lucide/svelte'
  import CollapsibleSection from './CollapsibleSection.svelte'
  import RemotePairingQr from './RemotePairingQr.svelte'
  import Tooltip from '../shared/Tooltip.svelte'
  import CustomSelect from '../shared/CustomSelect.svelte'
  import { remoteSession } from '../../lib/stores/remoteSession.svelte'
  import { prefs, setPref } from '../../lib/stores/preferences.svelte'
  import { showPreferences } from '../../lib/stores/dialogs.svelte'

  type GuardProfile = 'none' | 'destructive' | 'full'
  type NetworkInterface = { name: string; address: string; virtual: boolean }

  let interfaces = $state<NetworkInterface[]>([])
  let busy = $state(false)
  let errorMsg: string | null = $state(null)
  let copied = $state(false)
  let copiedTimer: ReturnType<typeof setTimeout> | null = null

  let status = $derived(remoteSession.status)
  let selectedInterface = $derived(prefs['remote.selectedInterface'] ?? '')
  let guardProfile: GuardProfile = $derived(
    (prefs['remote.actionGuard'] as GuardProfile) ?? 'destructive',
  )
  let pairingUrl = $derived(
    status.kind === 'waiting' || status.kind === 'peerArrived' ? status.pairingUrl : null,
  )
  let hostLabel = $derived.by(() => {
    if (
      status.kind === 'listening' ||
      status.kind === 'waiting' ||
      status.kind === 'peerArrived' ||
      status.kind === 'paired' ||
      status.kind === 'reconnecting'
    ) {
      return `${status.lanIp}:${status.port}`
    }
    return null
  })

  const interfaceGroups = $derived.by(() => {
    const auto = [{ value: '', label: 'Auto' }]
    const physical = interfaces
      .filter((i) => !i.virtual)
      .map((i) => ({ value: i.name, label: `${i.name} (${i.address})` }))
    const virtual = interfaces
      .filter((i) => i.virtual)
      .map((i) => ({ value: i.name, label: `${i.name} (${i.address}) virtual` }))
    const groups: Array<{ label: string; options: typeof auto }> = [
      { label: 'Auto', options: auto },
    ]
    if (physical.length) groups.push({ label: 'Physical', options: physical })
    if (virtual.length) groups.push({ label: 'Virtual', options: virtual })
    if (selectedInterface && !interfaces.some((i) => i.name === selectedInterface)) {
      groups.push({
        label: 'Unavailable',
        options: [{ value: selectedInterface, label: `${selectedInterface} (not ready)` }],
      })
    }
    return groups
  })

  const statusTone = $derived.by(() =>
    match(status.kind)
      .with('paired', () => 'success')
      .with('waiting', 'peerArrived', () => 'accent')
      .with('listening', 'reconnecting', 'starting', () => 'warning')
      .with('error', () => 'danger')
      .with('idle', () => 'muted')
      .exhaustive(),
  )

  const statusLabel = $derived.by(() =>
    match(status)
      .with({ kind: 'idle' }, () => 'Idle')
      .with({ kind: 'starting' }, () => 'Starting')
      .with({ kind: 'listening' }, () => 'Listening')
      .with({ kind: 'waiting' }, () => 'Pairing')
      .with({ kind: 'peerArrived' }, () => 'Approval needed')
      .with({ kind: 'paired' }, (s) => `Connected: ${s.deviceName}`)
      .with({ kind: 'reconnecting' }, () => 'Reconnecting')
      .with({ kind: 'error' }, () => 'Error')
      .exhaustive(),
  )

  onMount(() => {
    void loadInterfaces()
  })

  onDestroy(() => {
    if (copiedTimer) clearTimeout(copiedTimer)
  })

  async function loadInterfaces(): Promise<void> {
    interfaces = await window.api.remote.listNetworkInterfaces()
  }

  async function startPairing(): Promise<void> {
    if (busy) return
    busy = true
    errorMsg = null
    try {
      await window.api.remote.start()
      await loadInterfaces()
    } catch (e) {
      errorMsg = e instanceof Error ? e.message : String(e)
    } finally {
      busy = false
    }
  }

  async function stopSession(): Promise<void> {
    if (busy) return
    busy = true
    errorMsg = null
    try {
      await window.api.remote.stop()
    } catch (e) {
      errorMsg = e instanceof Error ? e.message : String(e)
    } finally {
      busy = false
    }
  }

  async function copyUrl(): Promise<void> {
    if (!pairingUrl) return
    try {
      await navigator.clipboard.writeText(pairingUrl)
      copied = true
      if (copiedTimer) clearTimeout(copiedTimer)
      copiedTimer = setTimeout(() => {
        copied = false
        copiedTimer = null
      }, 1500)
    } catch {
      copied = false
    }
  }

  function setInterface(name: string): void {
    setPref('remote.selectedInterface', name)
  }

  function setGuard(value: string): void {
    setPref('remote.actionGuard', value)
  }
</script>

<CollapsibleSection title="REMOTE" sectionKey="remote" borderTop>
  {#snippet headerExtra()}
    <Tooltip text="Remote Control settings">
      <button
        class="inline-flex items-center justify-center size-5 -my-1 border-0 bg-transparent text-text-faint cursor-pointer rounded-sm transition-colors duration-fast hover:bg-hover hover:text-text"
        aria-label="Open Remote Control settings"
        onclick={() => showPreferences('Remote Control')}
      >
        <Settings size={13} />
      </button>
    </Tooltip>
  {/snippet}

  <div class="px-3 flex flex-col gap-3">
    <div class="flex items-center gap-2 min-w-0">
      <span
        class="remote-dot size-2 rounded-full shrink-0"
        data-tone={statusTone}
        aria-hidden="true"
      ></span>
      <div class="min-w-0 flex-1">
        <div class="text-sm text-text truncate" title={statusLabel}>{statusLabel}</div>
        {#if hostLabel}
          <div class="text-2xs text-text-muted font-mono truncate" title={hostLabel}>
            {hostLabel}
          </div>
        {:else}
          <div class="text-2xs text-text-muted truncate">No active listener</div>
        {/if}
      </div>
    </div>

    {#if pairingUrl}
      <div class="flex justify-center p-2 bg-bg-input border border-border-subtle rounded-md">
        <RemotePairingQr url={pairingUrl} />
      </div>
      <button
        type="button"
        class="inline-flex items-center justify-center gap-1 w-full h-7 rounded-md border-0 bg-accent-bg text-accent-text text-xs font-medium cursor-pointer hover:bg-accent-bg-hover focus-visible:outline-2 focus-visible:outline-focus-ring focus-visible:outline-offset-1"
        onclick={copyUrl}
      >
        {#if copied}
          <Check size={13} />
          Copied
        {:else}
          <Copy size={13} />
          Copy URL
        {/if}
      </button>
    {/if}

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
        <span class="truncate">Trusted devices may reconnect.</span>
      </div>
    {/if}

    {#if errorMsg || status.kind === 'error'}
      <div role="alert" class="text-xs text-danger-text bg-danger-bg rounded-md px-2.5 py-2">
        {errorMsg ?? (status.kind === 'error' ? status.message : '')}
      </div>
    {/if}

    <div class="flex gap-1">
      {#if status.kind === 'idle' || status.kind === 'error' || status.kind === 'listening'}
        <button
          type="button"
          class="inline-flex items-center justify-center gap-1 flex-1 h-7 rounded-md border-0 bg-accent-bg text-accent-text text-xs font-medium cursor-pointer enabled:hover:bg-accent-bg-hover disabled:opacity-50 disabled:cursor-wait focus-visible:outline-2 focus-visible:outline-focus-ring focus-visible:outline-offset-1"
          disabled={busy}
          onclick={startPairing}
        >
          <Play size={13} />
          Pair
        </button>
      {:else}
        <button
          type="button"
          class="inline-flex items-center justify-center gap-1 flex-1 h-7 rounded-md border-0 bg-danger-bg text-danger-text text-xs font-medium cursor-pointer enabled:hover:bg-hover disabled:opacity-50 disabled:cursor-wait focus-visible:outline-2 focus-visible:outline-focus-ring focus-visible:outline-offset-1"
          disabled={busy}
          onclick={stopSession}
        >
          <Square size={12} />
          Stop
        </button>
      {/if}
    </div>

    <div class="flex flex-col gap-2 pt-2 border-t border-border-subtle">
      <label class="flex flex-col gap-1 text-2xs uppercase tracking-caps-tight text-text-faint">
        Adapter
        <CustomSelect value={selectedInterface} groups={interfaceGroups} onchange={setInterface} />
      </label>
      <label class="flex flex-col gap-1 text-2xs uppercase tracking-caps-tight text-text-faint">
        Guard
        <CustomSelect
          value={guardProfile}
          onchange={setGuard}
          options={[
            { value: 'destructive', label: 'Destructive' },
            { value: 'full', label: 'Full' },
            { value: 'none', label: 'None' },
          ]}
        />
      </label>
    </div>
  </div>
</CollapsibleSection>

<style>
  .remote-dot[data-tone='success'] {
    background: var(--color-success);
    box-shadow: 0 0 8px var(--color-success);
  }

  .remote-dot[data-tone='accent'] {
    background: var(--color-accent);
    box-shadow: 0 0 8px var(--color-accent);
  }

  .remote-dot[data-tone='warning'] {
    background: var(--color-warning);
    box-shadow: 0 0 8px var(--color-warning);
  }

  .remote-dot[data-tone='danger'] {
    background: var(--color-danger);
    box-shadow: 0 0 8px var(--color-danger);
  }

  .remote-dot[data-tone='muted'] {
    background: var(--color-text-muted);
  }
</style>
