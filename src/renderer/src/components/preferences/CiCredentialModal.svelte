<script lang="ts">
  import { onMount } from 'svelte'
  import { KeyRound, LoaderCircle, X } from '@lucide/svelte'
  import { cycleFocus } from '../../lib/a11y/focusTrap'
  import { replaceCiCredential } from '../../lib/ci/credentialUpdate'
  import { ipcErrorMessage } from '../../lib/ci/errors'
  import { githubTokenCreationUrl } from '../../lib/ci/githubToken'
  import { teamCityTokenCreationUrl } from '../../lib/ci/teamCityToken'
  import type { CiRepoConfigInfo } from '../../lib/ci/types'
  import { bumpCiCredentialTick, loadCiRepoConfig } from '../../lib/stores/ci.svelte'
  import { closeDialog, confirm } from '../../lib/stores/dialogs.svelte'
  import { addToast } from '../../lib/stores/toast.svelte'
  import TrackerProviderIcon from '../shared/TrackerProviderIcon.svelte'
  import CredentialStorageNote from './_partials/CredentialStorageNote.svelte'
  import { credentialStorageClause } from './_partials/credentialStorage'

  let {
    repoRoot,
    config,
    onClose = closeDialog,
    onUpdated,
  }: {
    repoRoot: string
    config: CiRepoConfigInfo
    onClose?: () => void
    onUpdated?: () => void
  } = $props()

  let containerEl: HTMLElement | undefined = $state()
  let tokenInputEl: HTMLInputElement | undefined = $state()
  let token = $state('')
  let submitting = $state(false)
  let saving = $state(false)
  let error = $state('')

  let isGitHub = $derived(config.provider === 'github-actions')
  let providerName = $derived(isGitHub ? 'GitHub Actions' : 'TeamCity')
  let target = $derived(
    config.provider === 'github-actions'
      ? `https://github.com/${config.repository}`
      : config.baseUrl,
  )
  let targetLabel = $derived(config.provider === 'github-actions' ? 'Repository' : 'Server')
  let canSave = $derived(token.trim().length > 0 && !submitting)

  onMount(() => {
    containerEl?.focus()
    tokenInputEl?.focus()
  })

  function requestClose(): void {
    if (!saving) onClose()
  }

  function handleKeydown(event: KeyboardEvent): void {
    if (event.key === 'Escape') {
      event.preventDefault()
      requestClose()
    } else if (event.key === 'Tab' && containerEl) {
      cycleFocus(containerEl, event)
    }
  }

  async function confirmTeamCityDestination(): Promise<boolean> {
    if (config.provider !== 'teamcity') return true
    const encryptionAvailable = await window.api
      .isCredentialEncryptionAvailable()
      .catch(() => false)
    const storage = credentialStorageClause(window.api.platform, encryptionAvailable)
    const insecure = config.baseUrl.startsWith('http://')
    return confirm({
      title: 'Confirm CI server address',
      message: `Send your TeamCity token to ${config.baseUrl}?`,
      details:
        `The token will be tested only against this address and stored ${storage} for this server-scoped TeamCity integration. The repository's shared CI configuration will not be changed.` +
        (insecure
          ? ' Warning: this is a plain http:// address - the token would travel unencrypted.'
          : ''),
      confirmLabel: 'Continue',
    })
  }

  function openTokenPage(): void {
    const url =
      config.provider === 'github-actions'
        ? githubTokenCreationUrl(config.repository)
        : teamCityTokenCreationUrl(config.baseUrl)
    void window.api.openExternal(url)
  }

  async function saveToken(): Promise<void> {
    const candidate = token.trim()
    if (!candidate || submitting) return
    submitting = true
    error = ''
    try {
      if (!(await confirmTeamCityDestination())) return
      saving = true
      await replaceCiCredential(window.api, repoRoot, config, candidate)
      bumpCiCredentialTick()
      await loadCiRepoConfig(repoRoot)
      token = ''
      addToast(`${providerName} token updated`, 'success')
      onUpdated?.()
      onClose()
    } catch (cause) {
      error = ipcErrorMessage(cause, `${providerName} rejected the token`)
    } finally {
      saving = false
      submitting = false
    }
  }
</script>

<!-- svelte-ignore a11y_no_static_element_interactions -->
<div
  class="fixed inset-0 z-overlay flex items-center justify-center bg-scrim"
  onkeydown={handleKeydown}
  onmousedown={requestClose}
>
  <!-- svelte-ignore a11y_click_events_have_key_events -->
  <div
    bind:this={containerEl}
    class="outline-none w-[520px] max-w-[92vw] rounded-xl border border-border bg-bg-overlay shadow-modal overflow-hidden"
    role="dialog"
    aria-modal="true"
    aria-labelledby="ci-credential-title"
    tabindex="-1"
    onmousedown={(event) => event.stopPropagation()}
  >
    <header class="px-6 py-4 border-b border-border-subtle flex items-start justify-between gap-3">
      <div class="min-w-0">
        <h2
          id="ci-credential-title"
          class="m-0 text-lg font-semibold text-text flex items-center gap-2"
        >
          <TrackerProviderIcon provider={isGitHub ? 'github' : 'teamcity'} size={18} />
          Update {providerName} token
        </h2>
        <p class="m-0 mt-1 text-xs text-text-muted">
          This changes only the credential stored on this machine.
        </p>
      </div>
      <button
        type="button"
        class="size-7 rounded-md border-0 bg-transparent text-text-muted cursor-pointer hover:bg-hover hover:text-text aria-disabled:opacity-50 aria-disabled:cursor-default"
        onclick={requestClose}
        aria-disabled={saving}
        aria-label="Close"><X size={16} /></button
      >
    </header>

    <form
      class="px-6 py-5 flex flex-col gap-4"
      onsubmit={(event) => {
        event.preventDefault()
        void saveToken()
      }}
    >
      <div
        class="flex items-start gap-2 rounded-lg border border-experimental-border bg-experimental-bg px-3 py-2 text-xs text-text-secondary leading-snug"
      >
        <KeyRound size={14} class="mt-px shrink-0 text-warning-text" />
        <span>
          The shared {isGitHub ? 'workflow list' : 'job list'} in
          <code class="font-mono">.canopy/config.json</code> will not be loaded or modified.
        </span>
      </div>

      <div class="flex flex-col gap-1">
        <span class="text-2xs font-semibold uppercase tracking-caps-tight text-text-faint">
          {targetLabel}
        </span>
        <div
          class="px-2.5 py-1.5 rounded-md border border-border bg-bg-input text-sm text-text break-all"
        >
          {target}
        </div>
      </div>

      <div class="flex flex-col gap-1">
        <div class="flex items-center justify-between gap-2">
          <label
            for="ci-replacement-token"
            class="text-2xs font-semibold uppercase tracking-caps-tight text-text-faint"
          >
            New access token
          </label>
          <button
            type="button"
            class="text-2xs text-accent-text bg-transparent border-0 p-0 cursor-pointer underline underline-offset-2 hover:text-accent"
            onclick={openTokenPage}
          >
            Generate token {isGitHub ? 'on GitHub' : 'in TeamCity'} →
          </button>
        </div>
        <input
          bind:this={tokenInputEl}
          id="ci-replacement-token"
          type="password"
          class="px-2.5 py-1.5 border border-border rounded-md bg-bg-input text-text text-sm outline-none focus:border-focus-ring"
          bind:value={token}
          autocomplete="off"
          placeholder={isGitHub ? 'Fine-grained token' : 'Enter token'}
          aria-describedby="ci-credential-storage"
        />
      </div>

      <div id="ci-credential-storage">
        <CredentialStorageNote
          provider={config.provider}
          baseUrl={config.provider === 'github-actions' ? target : config.baseUrl}
          sharingNote={false}
        />
      </div>

      <div class:sr-only={!error} class="text-xs text-danger-text break-words" role="alert">
        {error}
      </div>

      <footer class="flex justify-end gap-2 border-t border-border-subtle pt-3">
        <button
          type="button"
          class="px-3 py-1 rounded-md border border-border bg-transparent text-sm text-text-secondary cursor-pointer hover:bg-hover aria-disabled:opacity-50 aria-disabled:cursor-default"
          onclick={requestClose}
          aria-disabled={saving}>Cancel</button
        >
        <button
          type="submit"
          class="flex items-center justify-center gap-1.5 min-w-28 px-3 py-1 rounded-md border-0 bg-accent-bg text-accent-text text-sm cursor-pointer hover:bg-accent-bg-hover disabled:opacity-50 disabled:cursor-default disabled:hover:bg-accent-bg"
          disabled={!canSave}
          aria-busy={saving}
        >
          {#if saving}
            <LoaderCircle size={13} class="animate-spin-slow motion-reduce:animate-none" />
            Updating…
          {:else}
            Update token
          {/if}
        </button>
      </footer>
    </form>
  </div>
</div>
