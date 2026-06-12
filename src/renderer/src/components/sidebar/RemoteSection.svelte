<script lang="ts">
  import { onDestroy, onMount } from 'svelte'
  import { match } from 'ts-pattern'
  import { Check, Copy, Settings, Smartphone, Wifi } from '@lucide/svelte'
  import CollapsibleSection from './CollapsibleSection.svelte'
  import RemoteControls from './RemoteControls.svelte'
  import RemotePairingQr from './RemotePairingQr.svelte'
  import RemoteSelectField from './RemoteSelectField.svelte'
  import RemoteStatusSummary from './RemoteStatusSummary.svelte'
  import Tooltip from '../shared/Tooltip.svelte'
  import { remoteSession } from '../../lib/stores/remoteSession.svelte'
  import { prefs, setPref } from '../../lib/stores/preferences.svelte'
  import { showPreferences } from '../../lib/stores/dialogs.svelte'
  import {
    buildRemoteInterfaceGroups,
    type NetworkInterface,
  } from '../../lib/remote/interfaceOptions'

  type TrustedDevice = { deviceId: string }

  let interfaces = $state<NetworkInterface[]>([])
  let trustedDeviceCount = $state(0)
  let busy = $state(false)
  let errorMsg: string | null = $state(null)
  let copied = $state(false)
  let pairSetupOpen = $state(false)
  let copiedTimer: ReturnType<typeof setTimeout> | null = null

  let status = $derived(remoteSession.status)
  let selectedInterface = $derived(prefs['remote.selectedInterface'] ?? '')
  let hasSelectedInterface = $derived(selectedInterface.length > 0)
  let listenAllInterfaces = $derived(prefs['remote.listenAllInterfaces'] === 'true')
  let listenerScope = $derived(listenAllInterfaces ? 'all' : 'selected')
  let canListen = $derived(trustedDeviceCount > 0 && (listenAllInterfaces || hasSelectedInterface))
  let showQrAdapter = $derived(
    pairSetupOpen || status.kind === 'waiting' || status.kind === 'peerArrived',
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
      if (status.kind === 'listening' && status.lanIp === '0.0.0.0') {
        return `All adapters:${status.port}`
      }
      return `${status.lanIp}:${status.port}`
    }
    return null
  })

  const interfaceGroups = $derived(buildRemoteInterfaceGroups(interfaces, selectedInterface))
  const listenerOptions = [
    { value: 'selected', label: 'Selected adapter' },
    { value: 'all', label: 'All adapters' },
  ]

  const statusTone = $derived.by(() =>
    match(status.kind)
      .with('paired', () => 'var(--color-success)')
      .with('waiting', 'peerArrived', () => 'var(--color-accent)')
      .with('listening', 'reconnecting', 'starting', () => 'var(--color-warning)')
      .with('error', () => 'var(--color-danger)')
      .with('idle', () => 'var(--color-text-muted)')
      .exhaustive(),
  )
  let statusDotStyle = $derived(
    status.kind === 'idle'
      ? `background: ${statusTone};`
      : `background: ${statusTone}; box-shadow: 0 0 8px ${statusTone};`,
  )

  const statusLabel = $derived.by(() =>
    match(status)
      .with({ kind: 'idle' }, () => 'Idle')
      .with({ kind: 'starting' }, () => 'Starting')
      .with({ kind: 'listening' }, (s) =>
        s.lanIp === '0.0.0.0' ? 'Listening on all adapters' : 'Listening',
      )
      .with({ kind: 'waiting' }, () => 'Pairing')
      .with({ kind: 'peerArrived' }, () => 'Approval needed')
      .with({ kind: 'paired' }, (s) => `Connected: ${s.deviceName}`)
      .with({ kind: 'reconnecting' }, () => 'Disconnected - listening')
      .with({ kind: 'error' }, () => 'Error')
      .exhaustive(),
  )

  onMount(() => {
    void loadInterfaces()
    void loadTrustedDevices()
  })

  onDestroy(() => {
    if (copiedTimer) clearTimeout(copiedTimer)
  })

  $effect(() => {
    if (status.kind === 'idle' || status.kind === 'listening' || status.kind === 'paired') {
      void loadTrustedDevices()
    }
  })

  async function loadInterfaces(): Promise<void> {
    interfaces = await window.api.remote.listNetworkInterfaces()
  }

  async function loadTrustedDevices(): Promise<void> {
    const devices: TrustedDevice[] = await window.api.remote.listTrustedDevices()
    trustedDeviceCount = devices.length
  }

  async function startListening(): Promise<void> {
    if (busy) return
    if (trustedDeviceCount === 0) {
      errorMsg = 'Remember a device before starting listen mode.'
      return
    }
    if (!listenAllInterfaces && !hasSelectedInterface) {
      errorMsg = 'Select a network adapter or listen on all adapters.'
      return
    }
    busy = true
    errorMsg = null
    try {
      await window.api.remote.ensureListening()
      await loadInterfaces()
      await loadTrustedDevices()
    } catch (e) {
      errorMsg = e instanceof Error ? e.message : String(e)
    } finally {
      busy = false
    }
  }

  async function startPairing(): Promise<void> {
    if (busy) return
    pairSetupOpen = true
    if (!hasSelectedInterface) {
      errorMsg = 'Select a network adapter before pairing.'
      return
    }
    busy = true
    errorMsg = null
    try {
      await window.api.remote.start()
      pairSetupOpen = false
      await loadInterfaces()
      await loadTrustedDevices()
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
      pairSetupOpen = false
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
    if (name) errorMsg = null
  }

  function setListenerScope(value: string): void {
    setPref('remote.listenAllInterfaces', value === 'all' ? 'true' : 'false')
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
    <RemoteSelectField
      label="Listening on"
      tooltip="Trusted reconnect listener: selected adapter or all adapters. QR pairing still uses the QR adapter."
      value={listenerScope}
      onchange={setListenerScope}
      options={listenerOptions}
    />

    <RemoteStatusSummary {statusDotStyle} {statusLabel} {hostLabel} />

    {#if showQrAdapter}
      <RemoteSelectField
        label="QR adapter"
        tooltip="Address encoded into the QR code. Pick the adapter the phone can reach."
        value={selectedInterface}
        groups={interfaceGroups}
        onchange={setInterface}
      />
    {/if}

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
        <span class="truncate">
          {status.lanIp === '0.0.0.0'
            ? 'Trusted devices may reconnect on any adapter.'
            : 'Trusted devices may reconnect.'}
        </span>
      </div>
    {:else if showQrAdapter && !hasSelectedInterface}
      <div
        class="text-xs text-warning-text bg-bg-input border border-border-subtle rounded-md px-2.5 py-2"
      >
        Select an adapter before pairing.
      </div>
    {/if}

    {#if errorMsg || status.kind === 'error'}
      <div role="alert" class="text-xs text-danger-text bg-danger-bg rounded-md px-2.5 py-2">
        {errorMsg ?? (status.kind === 'error' ? status.message : '')}
      </div>
    {/if}

    <RemoteControls
      {status}
      {busy}
      {canListen}
      onStartListening={startListening}
      onStartPairing={startPairing}
      onStopSession={stopSession}
    />
  </div>
</CollapsibleSection>
