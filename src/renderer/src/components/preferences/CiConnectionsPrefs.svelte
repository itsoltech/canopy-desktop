<script lang="ts">
  import { onMount } from 'svelte'
  import { Plus } from '@lucide/svelte'
  import { confirm } from '../../lib/stores/dialogs.svelte'
  import { addToast } from '../../lib/stores/toast.svelte'
  import { bumpCiCredentialTick } from '../../lib/stores/ci.svelte'
  import { credentialRemovalMessage } from '../../lib/credentials/removal'
  import { teamCityTokenCreationUrl } from '../../lib/ci/teamCityToken'
  import PrefsSection from './_partials/PrefsSection.svelte'
  import CredentialStorageNote from './_partials/CredentialStorageNote.svelte'
  import CiServerForm from './_partials/CiServerForm.svelte'
  import CiConnectionRow from './_partials/CiConnectionRow.svelte'
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
      authenticationState: string
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

  function credentialIssue(server: (typeof servers)[number]): string {
    if (server.authenticationState === 'invalid') {
      return 'The stored token was rejected. Reconnect it or retry after correcting the token.'
    }
    const denied = server.capabilities.flatMap((capability) => {
      const result = server.verification[capability]
      return result?.state === 'denied'
        ? [`${capability}${result.reason ? `: ${result.reason}` : ''}`]
        : []
    })
    return denied.length > 0
      ? `Permission check failed for ${denied.join(', ')}. Check the token permissions; Canopy will retry on the next request.`
      : ''
  }

  let credentialIssueAnnouncement = $derived(
    servers
      .map((server) => {
        const issue = credentialIssue(server)
        return issue ? `${server.baseUrl}: ${issue}` : ''
      })
      .filter(Boolean)
      .join(' '),
  )

  // Mirrors the Jira/YouTrack "Generate →" affordance: once the address is typed,
  // jump straight to the server's token page.
  function openTokenPage(): void {
    if (!urlValid) return
    window.api.openExternal(teamCityTokenCreationUrl(normalizedUrl))
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
        bumpCiCredentialTick()
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
      let result: Awaited<ReturnType<typeof window.api.keychainDeleteCredentials>>
      try {
        result = await window.api.keychainDeleteCredentials(server.provider, server.baseUrl)
        bumpCiCredentialTick()
      } catch (e) {
        addToast(e instanceof Error ? e.message : 'Failed to remove credentials')
        return
      }
      await reloadServers()
      addToast(credentialRemovalMessage(result, 'CI connection removed'))
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
    <!-- Mounted before the async credential list arrives; only its text mutates. -->
    <div class="sr-only" role="status">{credentialIssueAnnouncement}</div>
    {#if servers.length === 0 && editing === null}
      <p class="text-sm text-text-faint m-0">No CI connections yet.</p>
    {/if}

    {#each servers as server (`${server.provider}:${server.baseUrl}`)}
      {@const credentialIssueText = credentialIssue(server)}
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
        <CiConnectionRow
          {server}
          credentialIssue={credentialIssueText}
          {removingUrl}
          onEdit={() => startEdit(server)}
          onRemove={() => removeServer(server)}
        />
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
