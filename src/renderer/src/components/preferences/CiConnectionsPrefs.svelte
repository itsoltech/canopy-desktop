<script lang="ts">
  import { onMount } from 'svelte'
  import { Plus, Trash2, Check, LoaderCircle } from '@lucide/svelte'
  import { confirm } from '../../lib/stores/dialogs.svelte'
  import { addToast } from '../../lib/stores/toast.svelte'
  import TrackerProviderIcon from '../shared/TrackerProviderIcon.svelte'
  import PrefsSection from './_partials/PrefsSection.svelte'
  import CredentialStorageNote from './_partials/CredentialStorageNote.svelte'
  import CiServerForm from './_partials/CiServerForm.svelte'
  import { credentialStorageClause } from './_partials/credentialStorage'

  // Your PERSONAL CI server connections — credentials are capability-scoped and locally bound.
  // The credential registry is the connection list; no global-config entry exists. Which build
  // configurations a repository uses lives in the repo's own .canopy/config.json,
  // managed from the CI/CD sidebar section — not here. Kept as a separate Settings
  // section from the Project management connections on purpose.

  let servers = $state<
    Array<{
      provider: string
      baseUrl: string
      username?: string
      capabilities: string[]
      verification: Record<string, { state: string; checkedAt: string; reason?: string }>
      bindings: string[]
    }>
  >([])
  let editing = $state<string | null>(null) // '__new__' or the server baseUrl
  let formUrl = $state('')
  let formToken = $state('')
  let trimmedFormToken = $derived(formToken.trim())
  let testing = $state(false)
  let testResult = $state<'success' | 'fail' | ''>('')
  // In-flight guards: the confirm dialogs inside save/remove yield to the event loop,
  // so a double-click would otherwise start two overlapping keychain writes.
  let savingServer = $state(false)
  // VISIBLE save-busy state — set only after the destination confirm resolves,
  // so "Saving…" never shows while the user is still deciding.
  let savingBusy = $state(false)
  let removingServer = $state(false)
  // Which row's Trash is busy — the flag alone would spin EVERY row's icon.
  let removingUrl = $state('')

  let normalizedUrl = $derived(formUrl.trim().replace(/\/$/, ''))
  let urlValid = $derived(/^https?:\/\/\S+$/i.test(normalizedUrl))

  onMount(reloadServers)

  async function reloadServers(): Promise<void> {
    try {
      const all = await window.api.keychainListCredentials()
      servers = all.filter(
        (connection) =>
          connection.provider === 'teamcity' || connection.provider === 'github-actions',
      )
    } catch {
      servers = []
    }
  }

  function startAdd(): void {
    editing = '__new__'
    formUrl = ''
    formToken = ''
    testResult = ''
  }

  function startEdit(server: { provider: string; baseUrl: string }): void {
    if (server.provider !== 'teamcity') return
    editing = server.baseUrl
    formUrl = server.baseUrl
    formToken = ''
    testResult = ''
  }

  function cancelEdit(): void {
    editing = null
    testResult = ''
  }

  // Mirrors the Jira/YouTrack "Generate →" affordance: once the address is typed,
  // jump straight to the server's token page.
  function openTokenPage(): void {
    if (!urlValid) return
    window.api.openExternal(`${normalizedUrl}/profile.html?item=accessTokens`)
  }

  // Same destination gate as the per-repo configurator (once per URL): the token is
  // sent on Test and stored on Save — either way the user names the host first.
  let acknowledgedUrl = $state('')

  async function confirmDestination(): Promise<boolean> {
    if (normalizedUrl === acknowledgedUrl) return true
    const encryptionAvailable = await window.api
      .isCredentialEncryptionAvailable()
      .catch(() => false)
    const storage = credentialStorageClause(window.api.platform, encryptionAvailable)
    const insecure = normalizedUrl.startsWith('http://')
    const ok = await confirm({
      title: 'Confirm CI server address',
      message: `Send your TeamCity token to ${normalizedUrl}?`,
      details:
        `The token will be sent only to this address and, when saved, stored ${storage} for this TeamCity server. Compatible repositories can bind to it locally; it is never written to any repository.` +
        (insecure
          ? ' Warning: this is a plain http:// address — the token would travel unencrypted.'
          : ''),
      confirmLabel: 'Continue',
    })
    if (ok) acknowledgedUrl = normalizedUrl
    return ok
  }

  async function testConnection(): Promise<void> {
    // Mirrors the form's aria-disabled — which does not stop clicks.
    if (!urlValid || !trimmedFormToken || testing) return
    if (!(await confirmDestination())) return
    testing = true
    testResult = ''
    try {
      await window.api.ciTestNewConnection(normalizedUrl, trimmedFormToken)
      testResult = 'success'
    } catch {
      testResult = 'fail'
    } finally {
      testing = false
    }
  }

  async function saveServer(): Promise<void> {
    if (!urlValid || !trimmedFormToken || savingServer) return
    savingServer = true
    try {
      if (!(await confirmDestination())) return
      savingBusy = true
      try {
        await window.api.keychainSetCredentials('teamcity', normalizedUrl, trimmedFormToken)
      } catch (e) {
        addToast(e instanceof Error ? e.message : 'Failed to save credentials')
        return
      }
      editing = null
      formToken = ''
      await reloadServers()
      addToast('CI connection saved')
    } finally {
      savingServer = false
      savingBusy = false
    }
  }

  async function removeServer(server: { provider: string; baseUrl: string }): Promise<void> {
    if (removingServer) return
    // Guard set before the await — that is what blocks a second confirm. The
    // VISIBLE busy state (removingUrl) waits for the answer: a spinner while the
    // user is still deciding describes work that has not started, and disabling
    // the button they just activated would blur it, so Cancel would drop focus.
    removingServer = true
    try {
      const ok = await confirm({
        title: 'Remove CI connection',
        message: `Remove your stored token for ${server.provider === 'github-actions' ? 'GitHub Actions' : 'TeamCity'} at ${server.baseUrl}?`,
        details:
          'Removes this CI binding on this machine. A credential still bound to another integration is retained; otherwise its secret is deleted.',
        confirmLabel: 'Remove connection',
        destructive: true,
      })
      if (!ok) return
      removingUrl = `${server.provider}:${server.baseUrl}`
      try {
        await window.api.keychainDeleteCredentials(server.provider, server.baseUrl)
      } catch (e) {
        addToast(e instanceof Error ? e.message : 'Failed to remove credentials')
        return
      }
      await reloadServers()
      addToast('CI connection removed')
    } finally {
      removingServer = false
      removingUrl = ''
    }
  }
</script>

<PrefsSection
  title="Connections & credentials"
  description="Personal CI tokens stored on this machine — TeamCity credentials are server-scoped; GitHub Actions credentials are repository-scoped"
>
  <div class="flex flex-col gap-2">
    {#if servers.length === 0 && editing === null}
      <p class="text-sm text-text-faint m-0">No CI connections yet.</p>
    {/if}

    {#each servers as server (`${server.provider}:${server.baseUrl}`)}
      {#if editing === server.baseUrl}
        <CiServerForm
          bind:url={formUrl}
          bind:token={formToken}
          isNew={false}
          {urlValid}
          {testing}
          {testResult}
          onCancel={cancelEdit}
          onTest={testConnection}
          saving={savingBusy}
          onSave={saveServer}
          onOpenTokenPage={openTokenPage}
        />
      {:else}
        <div class="flex items-center gap-1">
          <button
            type="button"
            class="flex-1 flex items-center gap-2 px-2.5 py-1.5 border border-border-subtle rounded-md bg-bg-input text-text text-sm font-inherit cursor-pointer text-left enabled:hover:border-border disabled:cursor-default min-w-0"
            onclick={() => startEdit(server)}
            disabled={server.provider !== 'teamcity'}
            title={server.provider === 'teamcity'
              ? 'Update the stored token for this server'
              : 'GitHub token — update it from a repository GitHub Actions configurator'}
          >
            <span
              class="inline-flex items-center shrink-0 text-text-muted"
              title={server.provider === 'github-actions' ? 'GitHub Actions' : 'TeamCity'}
            >
              <TrackerProviderIcon provider={server.provider} size={14} />
            </span>
            <span class="flex-1 text-text-secondary truncate" title={server.baseUrl}
              >{server.baseUrl}</span
            >
            <span
              class="text-2xs text-text-faint shrink-0"
              title={`Capabilities: ${server.capabilities.map((capability) => `${capability} (${server.verification[capability]?.state ?? 'unverified'})`).join(', ')}. Bindings: ${server.bindings.join(', ') || 'none'}`}
              >{server.provider === 'github-actions'
                ? 'Actions · repo scoped'
                : 'Builds · server scoped'}</span
            >
            <span
              class="flex items-center gap-1 text-2xs text-success shrink-0"
              title="Credentials saved"
            >
              <Check size={12} />
            </span>
          </button>
          <!-- aria-disabled: a real disabled makes ConfirmDialog's focus restore
               a no-op (.focus() on a disabled element does nothing), stranding
               the user on <body> after confirming. removeServer's guard blocks
               re-entry. -->
          <button
            type="button"
            class="flex items-center justify-center size-7 rounded-md bg-transparent border-0 text-text-muted cursor-pointer hover:bg-danger-bg hover:text-danger-text aria-disabled:opacity-50 aria-disabled:cursor-default aria-disabled:hover:bg-transparent aria-disabled:hover:text-text-muted"
            onclick={() => removeServer(server)}
            aria-disabled={removingUrl !== ''}
            aria-busy={removingUrl === `${server.provider}:${server.baseUrl}`}
            aria-label="Remove CI connection"
            title={removingUrl !== ''
              ? removingUrl === `${server.provider}:${server.baseUrl}`
                ? 'Removing…'
                : 'Disabled while another connection is being removed'
              : 'Remove the stored token for this server'}
          >
            {#if removingUrl === `${server.provider}:${server.baseUrl}`}
              <LoaderCircle size={12} class="animate-spin-slow motion-reduce:animate-none" />
            {:else}
              <Trash2 size={12} />
            {/if}
          </button>
        </div>
      {/if}
    {/each}

    {#if editing === '__new__'}
      <CiServerForm
        bind:url={formUrl}
        bind:token={formToken}
        isNew={true}
        {urlValid}
        {testing}
        {testResult}
        onCancel={cancelEdit}
        onTest={testConnection}
        saving={savingBusy}
        onSave={saveServer}
        onOpenTokenPage={openTokenPage}
      />
    {/if}

    {#if editing === null}
      <button
        type="button"
        class="self-start flex items-center gap-1 px-3 py-1 mt-1 rounded-md bg-border-subtle border border-border text-text-secondary text-sm font-inherit cursor-pointer hover:bg-active hover:text-text"
        onclick={startAdd}
        title="Add a CI server and your access token"
      >
        <Plus size={12} />
        <span>Add connection</span>
      </button>
    {/if}

    <div class="mt-1">
      <CredentialStorageNote
        provider={editing ? 'teamcity' : undefined}
        baseUrl={editing && urlValid ? normalizedUrl : undefined}
        sharingNote={false}
        stored={editing === null}
      />
    </div>
  </div>
</PrefsSection>
