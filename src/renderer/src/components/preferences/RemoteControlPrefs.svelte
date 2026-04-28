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

<div class="flex flex-col gap-4">
  <h3 class="text-[15px] font-semibold text-text m-0 inline-flex items-center gap-2">
    Remote Control
    <span
      class="inline-block px-2 py-0.5 text-[9px] font-bold uppercase tracking-[0.6px] text-warning bg-experimental-bg border border-experimental-border rounded-[10px] align-middle"
      title="This feature is in beta — expect rough edges">Beta</span
    >
  </h3>

  <p class="text-xs text-text-muted leading-normal">
    Control Canopy from another device on your local WiFi network — phone, tablet, or another
    laptop. Scan a QR code from the command palette to pair a device. Connections are end-to-end
    encrypted via WebRTC DTLS. Once you have a trusted device, the signaling server stays bound in
    the background on every app launch so that device can reconnect without you reopening the
    pairing modal — disable the toggle below or remove all trusted devices to stop this auto-listen
    behavior.
  </p>

  <div
    class="px-3 py-2.5 bg-experimental-bg border border-experimental-border rounded-lg text-xs text-text-secondary leading-normal"
  >
    <strong class="text-text block mb-1">Beta security notes:</strong>
    <ul class="m-0 pl-4">
      <li>
        The pairing handshake (token exchange, SDP offer/answer) travels over plain HTTP/WebSocket
        on the LAN — anyone with a packet sniffer on the same WiFi can observe it. Only the WebRTC
        data channels themselves are encrypted (DTLS).
      </li>
      <li class="mt-1">
        "Remember this device" uses the device's self-reported random ID for recognition — it's not
        cryptographic identity. Anyone who knows or guesses a trusted device's ID can skip the
        accept modal. Cryptographic challenge-response is planned for a future release; until then,
        avoid using this on untrusted networks.
      </li>
    </ul>
  </div>

  <label class="flex items-center gap-2 text-md text-text cursor-pointer">
    <CustomCheckbox checked={enabled} onchange={toggleEnabled} />
    <span>Enable remote control</span>
  </label>
  <div class="text-xs text-text-muted leading-normal pl-6 -mt-2">
    When disabled, the command palette does not expose the "Open Remote Connection" action, the
    signaling server is never bound (including listen mode), and trusted devices cannot reconnect
    until the toggle is re-enabled.
  </div>

  <fieldset
    class="flex flex-col gap-2 p-3 border border-border rounded-lg"
    class:opacity-50={!enabled}
    disabled={!enabled}
  >
    <legend class="text-xs font-semibold uppercase tracking-[0.4px] text-text-secondary px-1.5"
      >Action restrictions</legend
    >
    <label class="flex gap-2 items-start cursor-pointer text-md text-text">
      <input
        class="mt-[3px]"
        type="radio"
        name="remote-action-guard"
        value="none"
        checked={guardProfile === 'none'}
        onchange={() => setGuard('none')}
      />
      <span class="flex flex-col gap-0.5">
        <strong>None</strong>
        <small class="text-xs text-text-muted"
          >Paired device has full access without per-action prompts.</small
        >
      </span>
    </label>
    <label class="flex gap-2 items-start cursor-pointer text-md text-text">
      <input
        class="mt-[3px]"
        type="radio"
        name="remote-action-guard"
        value="destructive"
        checked={guardProfile === 'destructive'}
        onchange={() => setGuard('destructive')}
      />
      <span class="flex flex-col gap-0.5">
        <strong>Destructive only (recommended)</strong>
        <small class="text-xs text-text-muted"
          >Ask before kills, deletes, worktree removes and tab closes.</small
        >
      </span>
    </label>
    <label class="flex gap-2 items-start cursor-pointer text-md text-text">
      <input
        class="mt-[3px]"
        type="radio"
        name="remote-action-guard"
        value="full"
        checked={guardProfile === 'full'}
        onchange={() => setGuard('full')}
      />
      <span class="flex flex-col gap-0.5">
        <strong>Full</strong>
        <small class="text-xs text-text-muted"
          >Every action from the remote device requires confirmation on this desktop.</small
        >
      </span>
    </label>
  </fieldset>

  <div>
    <h4 class="text-md font-semibold text-text m-0 mb-1">Trusted Devices</h4>
    <p class="text-xs text-text-muted leading-normal">
      Devices you mark as "Remember this device" during pairing auto-connect on future sessions
      without showing the accept modal. Remove a device here to require manual approval again.
    </p>
    {#if loading}
      <p
        class="text-sm text-text-muted px-3.5 py-3 bg-bg-input border border-dashed border-border-subtle rounded-lg mt-2"
      >
        Loading…
      </p>
    {:else if trustedDevices.length === 0}
      <p
        class="text-sm text-text-muted px-3.5 py-3 bg-bg-input border border-dashed border-border-subtle rounded-lg mt-2"
      >
        No trusted devices yet.
      </p>
    {:else}
      <ul class="list-none p-0 m-0 mt-2 flex flex-col gap-1.5">
        {#each trustedDevices as device (device.deviceId)}
          <li
            class="flex items-center justify-between gap-3 px-3 py-2.5 bg-bg-input border border-border-subtle rounded-lg"
          >
            <div class="flex flex-col gap-0.5 min-w-0">
              <span class="text-md font-medium text-text">{device.name}</span>
              <span class="text-2xs text-text-muted">
                <code class="font-mono text-text-secondary">{device.deviceId.slice(0, 12)}…</code>
                · added {formatRelative(device.addedAt)}
                · last seen {formatRelative(device.lastSeen)}
              </span>
            </div>
            <button
              type="button"
              class="px-3 py-1 rounded-md bg-bg border border-border text-danger-text text-xs cursor-pointer flex-shrink-0 hover:bg-danger-bg hover:border-danger"
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
