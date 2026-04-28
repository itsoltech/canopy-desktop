<script lang="ts">
  import { onMount } from 'svelte'
  import { prefs, setPref } from '../../lib/stores/preferences.svelte'
  import { confirm } from '../../lib/stores/dialogs.svelte'
  import CustomCheckbox from '../shared/CustomCheckbox.svelte'

  type GuardProfile = 'none' | 'destructive' | 'full'

  let enabled = $derived(prefs['remote.enabled'] === 'true')
  let guardProfile: GuardProfile = $derived(
    (prefs['remote.actionGuard'] as GuardProfile) ?? 'destructive',
  )

  // Device shape matches what `window.api.remote.listTrustedDevices`
  // resolves to — kept inline so this component doesn't have to import
  // the ambient type declaration from `preload/index.d.ts`.
  type TrustedDevice = {
    deviceId: string
    name: string
    addedAt: string
    lastSeen: string
    publicKeyJwk: unknown
  }

  let trustedDevices = $state<TrustedDevice[]>([])
  let loading = $state(false)

  function toggleEnabled(): void {
    setPref('remote.enabled', enabled ? 'false' : 'true')
  }

  function setGuard(profile: GuardProfile): void {
    setPref('remote.actionGuard', profile)
  }

  async function loadTrustedDevices(): Promise<void> {
    loading = true
    try {
      trustedDevices = await window.api.remote.listTrustedDevices()
    } finally {
      loading = false
    }
  }

  async function removeDevice(deviceId: string, name: string): Promise<void> {
    const ok = await confirm({
      title: 'Remove Trusted Device',
      message: `Remove "${name}" from trusted devices? It will need manual approval to connect again.`,
      confirmLabel: 'Remove',
      destructive: true,
    })
    if (!ok) return
    try {
      await window.api.remote.removeTrustedDevice(deviceId)
      await loadTrustedDevices()
    } catch (e) {
      console.warn('[remote] removeTrustedDevice failed:', e)
    }
  }

  function formatRelative(iso: string): string {
    const then = new Date(iso).getTime()
    if (Number.isNaN(then)) return '—'
    const diff = Date.now() - then
    const minute = 60 * 1000
    const hour = 60 * minute
    const day = 24 * hour
    if (diff < minute) return 'just now'
    if (diff < hour) return `${Math.floor(diff / minute)}m ago`
    if (diff < day) return `${Math.floor(diff / hour)}h ago`
    return `${Math.floor(diff / day)}d ago`
  }

  onMount(() => {
    void loadTrustedDevices()
  })
</script>

<div class="section">
  <h3 class="section-title">
    Remote Control
    <span class="beta-badge" title="This feature is in beta — expect rough edges">Beta</span>
  </h3>

  <p class="hint-row intro">
    Control Canopy from another device on your local WiFi network — phone, tablet, or another
    laptop. Scan a QR code from the command palette to pair a device. Connections are end-to-end
    encrypted via WebRTC DTLS. Once you have a trusted device, the signaling server stays bound in
    the background on every app launch so that device can reconnect without you reopening the
    pairing modal — disable the toggle below or remove all trusted devices to stop this auto-listen
    behavior.
  </p>

  <div class="security-note">
    <strong>Beta security notes:</strong>
    <ul>
      <li>
        The pairing handshake (token exchange, SDP offer/answer) travels over plain HTTP/WebSocket
        on the LAN — anyone with a packet sniffer on the same WiFi can observe it. Only the WebRTC
        data channels themselves are encrypted (DTLS).
      </li>
      <li>
        "Remember this device" uses the device's self-reported random ID for recognition — it's not
        cryptographic identity. Anyone who knows or guesses a trusted device's ID can skip the
        accept modal. Cryptographic challenge-response is planned for a future release; until then,
        avoid using this on untrusted networks.
      </li>
    </ul>
  </div>

  <label class="checkbox-row">
    <CustomCheckbox checked={enabled} onchange={toggleEnabled} />
    <span>Enable remote control</span>
  </label>
  <div class="hint-row">
    When disabled, the command palette does not expose the "Open Remote Connection" action, the
    signaling server is never bound (including listen mode), and trusted devices cannot reconnect
    until the toggle is re-enabled.
  </div>

  <fieldset class="guard-group" class:disabled={!enabled} disabled={!enabled}>
    <legend>Action restrictions</legend>
    <label class="radio-row">
      <input
        type="radio"
        name="remote-action-guard"
        value="none"
        checked={guardProfile === 'none'}
        onchange={() => setGuard('none')}
      />
      <span>
        <strong>None</strong>
        <small>Paired device has full access without per-action prompts.</small>
      </span>
    </label>
    <label class="radio-row">
      <input
        type="radio"
        name="remote-action-guard"
        value="destructive"
        checked={guardProfile === 'destructive'}
        onchange={() => setGuard('destructive')}
      />
      <span>
        <strong>Destructive only (recommended)</strong>
        <small>Ask before kills, deletes, worktree removes and tab closes.</small>
      </span>
    </label>
    <label class="radio-row">
      <input
        type="radio"
        name="remote-action-guard"
        value="full"
        checked={guardProfile === 'full'}
        onchange={() => setGuard('full')}
      />
      <span>
        <strong>Full</strong>
        <small>Every action from the remote device requires confirmation on this desktop.</small>
      </span>
    </label>
  </fieldset>

  <div class="trusted-devices">
    <h4>Trusted Devices</h4>
    <p class="hint-row muted">
      Devices you mark as "Remember this device" during pairing auto-connect on future sessions
      without showing the accept modal. Remove a device here to require manual approval again.
    </p>
    {#if loading}
      <p class="devices-empty">Loading…</p>
    {:else if trustedDevices.length === 0}
      <p class="devices-empty">No trusted devices yet.</p>
    {:else}
      <ul class="device-list">
        {#each trustedDevices as device (device.deviceId)}
          <li class="device-row">
            <div class="device-info">
              <span class="device-name">{device.name}</span>
              <span class="device-meta">
                <code class="device-id">{device.deviceId.slice(0, 12)}…</code>
                · added {formatRelative(device.addedAt)}
                · last seen {formatRelative(device.lastSeen)}
              </span>
            </div>
            <button
              type="button"
              class="device-remove"
              onclick={() => removeDevice(device.deviceId, device.name)}
              title="Remove trusted device"
            >
              Remove
            </button>
          </li>
        {/each}
      </ul>
    {/if}
  </div>
</div>

<style>
  .section {
    display: flex;
    flex-direction: column;
    gap: 16px;
  }

  .section-title {
    font-size: 15px;
    font-weight: 600;
    color: var(--color-text);
    margin: 0;
    display: inline-flex;
    align-items: center;
    gap: 8px;
  }

  .beta-badge {
    display: inline-block;
    padding: 2px 8px;
    font-size: 9px;
    font-weight: 700;
    text-transform: uppercase;
    letter-spacing: 0.6px;
    color: var(--color-warning);
    background: color-mix(in srgb, var(--color-warning) 15%, transparent);
    border: 1px solid color-mix(in srgb, var(--color-warning) 40%, transparent);
    border-radius: 10px;
    vertical-align: middle;
  }

  .checkbox-row {
    display: flex;
    align-items: center;
    gap: 8px;
    font-size: 13px;
    color: var(--color-text);
    cursor: pointer;
  }

  .hint-row {
    font-size: 11px;
    color: var(--color-text-muted);
    line-height: 1.5;
    padding-left: 24px;
    margin-top: -8px;
  }

  .hint-row.intro {
    padding-left: 0;
    margin-top: 0;
  }

  .hint-row.muted {
    padding-left: 0;
    margin-top: 0;
  }

  .security-note {
    padding: 10px 12px;
    background: color-mix(in srgb, var(--color-warning) 8%, transparent);
    border: 1px solid color-mix(in srgb, var(--color-warning) 30%, transparent);
    border-radius: 6px;
    font-size: 11px;
    color: var(--color-text-secondary);
    line-height: 1.5;
  }

  .security-note strong {
    color: var(--color-text);
    display: block;
    margin-bottom: 4px;
  }

  .security-note ul {
    margin: 0;
    padding-left: 16px;
  }

  .security-note li + li {
    margin-top: 4px;
  }

  .guard-group {
    display: flex;
    flex-direction: column;
    gap: 8px;
    padding: 12px;
    border: 1px solid var(--color-border);
    border-radius: 6px;
  }

  .guard-group.disabled {
    opacity: 0.5;
  }

  .guard-group legend {
    font-size: 11px;
    font-weight: 600;
    text-transform: uppercase;
    letter-spacing: 0.4px;
    color: var(--color-text-secondary);
    padding: 0 6px;
  }

  .radio-row {
    display: flex;
    gap: 8px;
    align-items: flex-start;
    cursor: pointer;
    font-size: 13px;
    color: var(--color-text);
  }

  .radio-row input[type='radio'] {
    margin-top: 3px;
  }

  .radio-row span {
    display: flex;
    flex-direction: column;
    gap: 2px;
  }

  .radio-row small {
    font-size: 11px;
    color: var(--color-text-muted);
  }

  .trusted-devices h4 {
    font-size: 13px;
    font-weight: 600;
    color: var(--color-text);
    margin: 0 0 4px;
  }

  .devices-empty {
    font-size: 12px;
    color: var(--color-text-muted);
    padding: 12px 14px;
    background: var(--color-bg-input);
    border: 1px dashed var(--color-border-subtle);
    border-radius: 6px;
    margin: 8px 0 0;
  }

  .device-list {
    list-style: none;
    padding: 0;
    margin: 8px 0 0;
    display: flex;
    flex-direction: column;
    gap: 6px;
  }

  .device-row {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 12px;
    padding: 10px 12px;
    background: var(--color-bg-input);
    border: 1px solid var(--color-border-subtle);
    border-radius: 6px;
  }

  .device-info {
    display: flex;
    flex-direction: column;
    gap: 2px;
    min-width: 0;
  }

  .device-name {
    font-size: 13px;
    font-weight: 500;
    color: var(--color-text);
  }

  .device-meta {
    font-size: 10px;
    color: var(--color-text-muted);
  }

  .device-id {
    font-family: ui-monospace, 'SF Mono', Menlo, monospace;
    color: var(--color-text-secondary);
  }

  .device-remove {
    all: unset;
    padding: 5px 12px;
    border-radius: 4px;
    background: var(--color-bg);
    border: 1px solid var(--color-border);
    color: var(--color-danger-text);
    font-size: 11px;
    cursor: pointer;
    flex-shrink: 0;
  }

  .device-remove:hover {
    background: var(--color-danger-bg);
    border-color: var(--color-danger);
  }
</style>
