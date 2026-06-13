<script lang="ts">
  import { onMount } from 'svelte'
  import { match } from 'ts-pattern'
  import { Settings } from '@lucide/svelte'
  import CollapsibleSection from './CollapsibleSection.svelte'
  import RemoteControls from './RemoteControls.svelte'
  import RemotePairingQr from './RemotePairingQr.svelte'
  import RemoteSelectField from './RemoteSelectField.svelte'
  import RemoteSessionNotice from './RemoteSessionNotice.svelte'
  import RemoteStatusSummary from './RemoteStatusSummary.svelte'
  import Tooltip from '../shared/Tooltip.svelte'
  import { remoteSession } from '../../lib/stores/remoteSession.svelte'
  import { prefs, setPref } from '../../lib/stores/preferences.svelte'
  import { showPreferences } from '../../lib/stores/dialogs.svelte'
  import {
    applyRemoteListenerPref,
    buildRemoteInterfaceGroups,
    buildRemoteListenerGroups,
    formatRemoteInterfaceLabel,
    REMOTE_LISTEN_ALL_VALUE,
    type NetworkInterface,
  } from '../../lib/remote/interfaceOptions'

  let interfaces = $state<NetworkInterface[]>([])
  let busy = $state(false)
  let errorMsg: string | null = $state(null)
  let pairSetupOpen = $state(false)
  let qrInterface = $state('')

  let status = $derived(remoteSession.status)
  let listenerInterface = $derived(prefs['remote.selectedInterface'] ?? '')
  let hasListenerInterface = $derived(listenerInterface.length > 0)
  let listenAllInterfaces = $derived(prefs['remote.listenAllInterfaces'] === 'true')
  let listenerValue = $derived(listenAllInterfaces ? REMOTE_LISTEN_ALL_VALUE : listenerInterface)
  let canListen = $derived(listenAllInterfaces || hasListenerInterface)
  let pairingUrl = $derived(
    status.kind === 'waiting' || status.kind === 'peerArrived' ? status.pairingUrl : null,
  )
  let showQrAdapter = $derived(pairSetupOpen || !!pairingUrl)
  let hasQrInterface = $derived(qrInterface.length > 0)
  let pairingActive = $derived(
    pairSetupOpen || status.kind === 'waiting' || status.kind === 'peerArrived',
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

  const qrInterfaceGroups = $derived(buildRemoteInterfaceGroups(interfaces, qrInterface))
  const listenerGroups = $derived(buildRemoteListenerGroups(interfaces, listenerInterface))
  const qrInterfaceLabel = $derived(formatRemoteInterfaceLabel(interfaces, qrInterface))

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
  })

  $effect(() => {
    if (status.kind === 'idle' || status.kind === 'listening' || status.kind === 'paired')
      void loadInterfaces()
  })

  async function loadInterfaces(): Promise<void> {
    interfaces = await window.api.remote.listNetworkInterfaces()
  }

  async function startListening(): Promise<void> {
    if (busy) return
    if (!listenAllInterfaces && !hasListenerInterface) {
      errorMsg = 'Select a network adapter or listen on all adapters.'
      return
    }
    busy = true
    errorMsg = null
    try {
      await window.api.remote.ensureListening({ allowWithoutTrusted: true })
      await loadInterfaces()
    } catch (e) {
      errorMsg = e instanceof Error ? e.message : String(e)
    } finally {
      busy = false
    }
  }

  async function startPairing(): Promise<void> {
    if (busy) return
    if (!pairSetupOpen && !pairingUrl) {
      pairSetupOpen = true
      qrInterface = ''
      errorMsg = null
      return
    }
    if (!qrInterface) {
      errorMsg = null
      return
    }
    await startPairingWithInterface(qrInterface)
  }

  async function startPairingWithInterface(interfaceName: string): Promise<void> {
    if (busy) return
    busy = true
    errorMsg = null
    try {
      await window.api.remote.start(interfaceName)
      pairSetupOpen = false
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
      pairSetupOpen = false
      qrInterface = ''
    } catch (e) {
      errorMsg = e instanceof Error ? e.message : String(e)
    } finally {
      busy = false
    }
  }

  async function cancelPairing(): Promise<void> {
    if (busy) return
    if (!pairingUrl && (pairSetupOpen || status.kind === 'listening')) {
      pairSetupOpen = false
      qrInterface = ''
      errorMsg = null
      return
    }
    busy = true
    errorMsg = null
    try {
      await window.api.remote.stop()
      pairSetupOpen = false
      qrInterface = ''
      await window.api.remote.ensureListening({ allowWithoutTrusted: true })
      await loadInterfaces()
    } catch (e) {
      errorMsg = e instanceof Error ? e.message : String(e)
    } finally {
      busy = false
    }
  }

  function setInterface(name: string): void {
    qrInterface = name
    if (name) errorMsg = null
    if (name && pairSetupOpen && !pairingUrl) {
      void startPairingWithInterface(name)
    }
  }

  function setListener(value: string): void {
    applyRemoteListenerPref(value, setPref)
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
      tooltip="This setting decides where Canopy listens for connections from trusted devices. You can choose a specific adapter or all active adapters."
      value={listenerValue}
      onchange={setListener}
      groups={listenerGroups}
    />
    <RemoteStatusSummary {statusDotStyle} {statusLabel} {hostLabel} />

    {#if pairSetupOpen && !pairingUrl}
      <RemoteSelectField
        label="Pick an adapter to generate the QR code"
        tooltip="Address will be encoded into the QR code. Pick the adapter the phone can reach."
        value={qrInterface}
        groups={qrInterfaceGroups}
        onchange={setInterface}
      />
    {:else if pairingUrl}
      <div class="flex flex-col gap-1">
        <div class="text-2xs uppercase tracking-caps-tight text-text-faint">
          Pick an adapter to generate the QR code
        </div>
        <div
          class="text-xs text-text-secondary bg-bg-input border border-border-subtle rounded-md px-2.5 py-2 truncate"
          title={qrInterfaceLabel}
        >
          {qrInterfaceLabel}
        </div>
      </div>
    {/if}

    {#if pairingUrl}
      <div class="flex justify-center p-2 bg-bg-input border border-border-subtle rounded-md">
        <RemotePairingQr url={pairingUrl} />
      </div>
    {/if}

    <RemoteSessionNotice {status} {showQrAdapter} {hasQrInterface} />

    {#if errorMsg || status.kind === 'error'}
      <div role="alert" class="text-xs text-danger-text bg-danger-bg rounded-md px-2.5 py-2">
        {errorMsg ?? (status.kind === 'error' ? status.message : '')}
      </div>
    {/if}

    <RemoteControls
      {status}
      {busy}
      {canListen}
      {pairingActive}
      onStartListening={startListening}
      onStartPairing={startPairing}
      onCancelPairing={cancelPairing}
      onStopSession={stopSession}
    />
  </div>
</CollapsibleSection>
