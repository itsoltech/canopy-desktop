<script lang="ts">
  import { onMount } from 'svelte'
  import { ShieldAlert, Trash2 } from '@lucide/svelte'
  import { prefs, setPref } from '../../lib/stores/preferences.svelte'
  import { confirm } from '../../lib/stores/dialogs.svelte'
  import CustomCheckbox from '../shared/CustomCheckbox.svelte'
  import CustomRadio from '../shared/CustomRadio.svelte'
  import PrefsSection from './_partials/PrefsSection.svelte'
  import PrefsRow from './_partials/PrefsRow.svelte'

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
      title: 'Remove trusted device',
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

<div class="flex flex-col gap-7">
  <div
    class="flex items-start gap-3 p-3 rounded-md bg-experimental-bg border border-experimental-border"
  >
    <ShieldAlert size={16} class="shrink-0 text-warning-text mt-0.5" />
    <div class="flex flex-col gap-1.5 text-xs text-text-secondary leading-snug">
      <p class="m-0">
        <strong class="text-text font-semibold">Beta security notes.</strong>
        Pairing handshakes (token exchange, SDP offer/answer) travel over plain HTTP/WebSocket on the
        LAN — anyone with a packet sniffer on the same WiFi can observe them. Only the WebRTC data channels
        themselves are encrypted (DTLS).
      </p>
      <p class="m-0">
        "Remember this device" uses the device's self-reported random ID for recognition — it is not
        cryptographic identity. Anyone who knows or guesses a trusted device's ID can skip the
        accept modal. Cryptographic challenge-response is planned for a future release; until then,
        avoid using this on untrusted networks.
      </p>
    </div>
  </div>

  <PrefsSection
    title="Remote control"
    description="Control Canopy from another device on your local network. Connections use end-to-end encrypted WebRTC DTLS data channels."
  >
    <PrefsRow
      label="Enable remote control"
      help="When disabled, the command palette hides the Open Remote Connection action, the signaling server is never bound, and trusted devices cannot reconnect."
      search="remote control enable signaling pair phone tablet"
      badge={{ text: 'Beta', tone: 'warning' }}
    >
      <CustomCheckbox checked={enabled} onchange={toggleEnabled} />
    </PrefsRow>
  </PrefsSection>

  <PrefsSection
    title="Action restrictions"
    description="What the paired device may do without per-action approval on this desktop"
  >
    <div class="flex flex-col" class:opacity-50={!enabled} class:pointer-events-none={!enabled}>
      <PrefsRow
        label="None"
        help="Paired device has full access without per-action prompts"
        search="remote guard none full-access"
      >
        <CustomRadio
          checked={guardProfile === 'none'}
          onchange={() => setGuard('none')}
          disabled={!enabled}
        />
      </PrefsRow>
      <PrefsRow
        label="Destructive only"
        help="Ask before kills, deletes, worktree removes, and tab closes"
        search="remote guard destructive recommended default"
        badge={{ text: 'Recommended', tone: 'accent' }}
      >
        <CustomRadio
          checked={guardProfile === 'destructive'}
          onchange={() => setGuard('destructive')}
          disabled={!enabled}
        />
      </PrefsRow>
      <PrefsRow
        label="Full"
        help="Every action from the remote device requires confirmation on this desktop"
        search="remote guard full strict every action"
      >
        <CustomRadio
          checked={guardProfile === 'full'}
          onchange={() => setGuard('full')}
          disabled={!enabled}
        />
      </PrefsRow>
    </div>
  </PrefsSection>

  <PrefsSection
    title="Trusted devices"
    description="Devices marked as remembered during pairing reconnect automatically. Remove a device here to require manual approval again."
  >
    {#if loading}
      <p
        class="text-sm text-text-muted px-3 py-3 bg-bg-input rounded-md border border-border-subtle"
      >
        Loading…
      </p>
    {:else if trustedDevices.length === 0}
      <p
        class="text-sm text-text-muted px-3 py-3 bg-bg-input rounded-md border border-border-subtle"
      >
        No trusted devices yet.
      </p>
    {:else}
      <ul role="list" class="m-0 p-0 flex flex-col gap-1">
        {#each trustedDevices as device (device.deviceId)}
          <li
            class="flex items-center justify-between gap-3 px-3 py-2 bg-bg-input border border-border-subtle rounded-md"
          >
            <div class="flex flex-col gap-0.5 min-w-0">
              <span class="text-md text-text truncate">{device.name}</span>
              <span class="text-2xs text-text-muted">
                <code class="font-mono text-text-secondary">{device.deviceId.slice(0, 12)}…</code>
                · added {formatRelative(device.addedAt)} · last seen {formatRelative(
                  device.lastSeen,
                )}
              </span>
            </div>
            <button
              type="button"
              class="flex items-center justify-center size-7 rounded-md bg-transparent border-0 text-text-muted cursor-pointer shrink-0 hover:bg-danger-bg hover:text-danger-text"
              onclick={() => removeDevice(device.deviceId, device.name)}
              aria-label="Remove {device.name}"
              title="Remove"
            >
              <Trash2 size={13} />
            </button>
          </li>
        {/each}
      </ul>
    {/if}
  </PrefsSection>
</div>
