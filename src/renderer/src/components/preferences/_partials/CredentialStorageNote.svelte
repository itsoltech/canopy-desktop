<script lang="ts">
  import { onMount } from 'svelte'
  import { Lock, ShieldAlert } from '@lucide/svelte'
  import { credentialStorageMechanism } from './credentialStorage'
  import { providerLabel } from '../../../lib/taskTracker/providerLabel'

  // Shown before credentials are saved (future tense). Credentials are encrypted at rest via
  // Electron safeStorage (DPAPI / Keychain / keyring) in Canopy's local DB — never written into any
  // file in the repositories. When a specific connection is in context, its provider + URL are named.
  // `sharingNote: false` drops the "all connections … will use those credentials" sentence when the
  // surrounding UI already says it.
  // `stored: true` switches to present tense — the note then describes credentials that already
  // exist rather than ones about to be saved.
  let {
    provider,
    baseUrl,
    sharingNote = true,
    stored = false,
  }: { provider?: string; baseUrl?: string; sharingNote?: boolean; stored?: boolean } = $props()

  let encryptionAvailable = $state(true)
  let checked = $state(false)
  const mechanism = credentialStorageMechanism(window.api.platform)
  let specific = $derived(!!(provider && baseUrl))
  let providerName = $derived(provider ? providerLabel(provider) : '')

  onMount(async () => {
    try {
      encryptionAvailable = await window.api.isCredentialEncryptionAvailable()
    } catch {
      encryptionAvailable = false
    } finally {
      checked = true
    }
  })
</script>

{#if checked && !encryptionAvailable}
  <div
    class="flex items-start gap-2 px-2.5 py-2 rounded-md bg-warning-bg border border-warning-text text-xs text-warning-text leading-snug"
  >
    <ShieldAlert size={14} class="shrink-0 mt-px" aria-hidden="true" />
    <div class="flex flex-col gap-1">
      {#if sharingNote}
        <span>
          {#if specific}All connections to <strong>{providerName}</strong> at
            <strong>{baseUrl}</strong>
            across your projects will use those credentials.{:else}All connections using the same
            provider + URL across your projects will use those credentials.{/if}
        </span>
      {/if}
      <span>
        Your credentials {stored ? 'are' : 'will be'} stored <strong>unencrypted</strong> in
        Canopy's <strong>local database on this machine</strong> (no OS keyring is available). Credentials
        are never written in the files within the repositories.
      </span>
    </div>
  </div>
{:else}
  <div
    class="flex items-start gap-2 px-2.5 py-2 rounded-md bg-success-bg border border-success-text text-xs text-success-text leading-snug"
  >
    <Lock size={14} class="shrink-0 mt-px" aria-hidden="true" />
    <div class="flex flex-col gap-1">
      {#if sharingNote}
        <span>
          {#if specific}All connections to <strong>{providerName}</strong> at
            <strong>{baseUrl}</strong>
            across your projects will use those credentials.{:else}All connections using the same
            provider + URL across your projects will use those credentials.{/if}
        </span>
      {/if}
      <span>
        Your credentials {stored ? 'are' : 'will be'} encrypted with {mechanism} and stored in Canopy's
        <strong>local database on this machine</strong>. Credentials are never written in the files
        within the repositories.
      </span>
    </div>
  </div>
{/if}
