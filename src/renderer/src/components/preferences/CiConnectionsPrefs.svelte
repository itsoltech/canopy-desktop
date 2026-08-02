<script lang="ts">
  import { onMount } from 'svelte'
  import { Plus, Trash2, Check } from '@lucide/svelte'
  import { confirm } from '../../lib/stores/dialogs.svelte'
  import { addToast } from '../../lib/stores/toast.svelte'
  import TrackerProviderIcon from '../shared/TrackerProviderIcon.svelte'
  import PrefsSection from './_partials/PrefsSection.svelte'
  import CredentialStorageNote from './_partials/CredentialStorageNote.svelte'
  import CiServerForm from './_partials/CiServerForm.svelte'
  import { credentialStorageClause } from './_partials/credentialStorage'

  // Your PERSONAL CI server connections — tokens keyed provider+URL in the keychain
  // (the keychain IS the connection list; no global-config entry exists). Which build
  // configurations a repository uses lives in the repo's own .canopy/config.json,
  // managed from the CI/CD sidebar section — not here. Kept as a separate Settings
  // section from the Project management connections on purpose.

  let servers = $state<Array<{ baseUrl: string; username?: string }>>([])
  let editing = $state<string | null>(null) // '__new__' or the server baseUrl
  let formUrl = $state('')
  let formToken = $state('')
  let testing = $state(false)
  let testResult = $state<'success' | 'fail' | ''>('')
  // In-flight guards: the confirm dialogs inside save/remove yield to the event loop,
  // so a double-click would otherwise start two overlapping keychain writes.
  let savingServer = $state(false)
  let removingServer = $state(false)

  let normalizedUrl = $derived(formUrl.trim().replace(/\/$/, ''))
  let urlValid = $derived(/^https?:\/\/\S+$/i.test(normalizedUrl))

  onMount(reloadServers)

  async function reloadServers(): Promise<void> {
    try {
      const all = await window.api.keychainListCredentials()
      servers = all.filter((c) => c.provider === 'teamcity')
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

  function startEdit(server: { baseUrl: string }): void {
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
        `The token will be sent only to this address and, when saved, stored ${storage}, keyed by provider + URL and used by every repository that configures this CI server — never written to any repository.` +
        (insecure
          ? ' Warning: this is a plain http:// address — the token would travel unencrypted.'
          : ''),
      confirmLabel: 'Continue',
    })
    if (ok) acknowledgedUrl = normalizedUrl
    return ok
  }

  async function testConnection(): Promise<void> {
    if (!urlValid || !formToken) return
    if (!(await confirmDestination())) return
    testing = true
    testResult = ''
    try {
      await window.api.ciTestNewConnection(normalizedUrl, formToken)
      testResult = 'success'
    } catch {
      testResult = 'fail'
    } finally {
      testing = false
    }
  }

  async function saveServer(): Promise<void> {
    if (!urlValid || !formToken || savingServer) return
    savingServer = true
    try {
      if (!(await confirmDestination())) return
      try {
        await window.api.keychainSetCredentials('teamcity', normalizedUrl, formToken)
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
    }
  }

  async function removeServer(server: { baseUrl: string }): Promise<void> {
    if (removingServer) return
    removingServer = true
    try {
      await doRemoveServer(server)
    } finally {
      removingServer = false
    }
  }

  async function doRemoveServer(server: { baseUrl: string }): Promise<void> {
    const ok = await confirm({
      title: 'Remove CI connection',
      message: `Remove your stored token for TeamCity at ${server.baseUrl}?`,
      details:
        'Clears the token on this machine only. Repositories that configure this server will show a reconnect hint until a new token is saved.',
      confirmLabel: 'Remove connection',
      destructive: true,
    })
    if (!ok) return
    try {
      await window.api.keychainDeleteCredentials('teamcity', server.baseUrl)
    } catch (e) {
      addToast(e instanceof Error ? e.message : 'Failed to remove credentials')
      return
    }
    await reloadServers()
    addToast('CI connection removed')
  }
</script>

<PrefsSection
  title="Connections & credentials"
  description="Your personal CI server connections and tokens stored on this machine — credentials are keyed by provider + URL and shared across your projects"
>
  <div class="flex flex-col gap-2">
    {#if servers.length === 0 && editing === null}
      <p class="text-sm text-text-faint m-0">No CI connections yet.</p>
    {/if}

    {#each servers as server (server.baseUrl)}
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
          onSave={saveServer}
          onOpenTokenPage={openTokenPage}
        />
      {:else}
        <div class="flex items-center gap-1">
          <button
            type="button"
            class="flex-1 flex items-center gap-2 px-2.5 py-1.5 border border-border-subtle rounded-md bg-bg-input text-text text-sm font-inherit cursor-pointer text-left hover:border-border min-w-0"
            onclick={() => startEdit(server)}
            title="Update the stored token for this server"
          >
            <span class="inline-flex items-center shrink-0 text-text-muted" title="TeamCity">
              <TrackerProviderIcon provider="teamcity" size={14} />
            </span>
            <span class="flex-1 text-text-secondary truncate" title={server.baseUrl}
              >{server.baseUrl}</span
            >
            <span
              class="flex items-center gap-1 text-2xs text-success shrink-0"
              title="Credentials saved"
            >
              <Check size={12} />
            </span>
          </button>
          <button
            type="button"
            class="flex items-center justify-center size-7 rounded-md bg-transparent border-0 text-text-muted cursor-pointer hover:bg-danger-bg hover:text-danger-text"
            onclick={() => removeServer(server)}
            aria-label="Remove CI connection"
            title="Remove the stored token for this server"
          >
            <Trash2 size={12} />
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
