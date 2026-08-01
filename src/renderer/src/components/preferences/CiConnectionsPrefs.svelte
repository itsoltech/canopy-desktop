<script lang="ts">
  import { onMount } from 'svelte'
  import { Plus, Trash2, Check, X, ServerCog } from '@lucide/svelte'
  import { confirm } from '../../lib/stores/dialogs.svelte'
  import { addToast } from '../../lib/stores/toast.svelte'
  import CustomSelect from '../shared/CustomSelect.svelte'
  import PrefsSection from './_partials/PrefsSection.svelte'
  import CredentialStorageNote from './_partials/CredentialStorageNote.svelte'
  import { credentialStorageClause } from './_partials/credentialStorage'

  // Your PERSONAL CI server connections — tokens keyed provider+URL in the keychain
  // (the keychain IS the connection list; no global-config entry exists). Which build
  // configurations a repository uses lives in the repo's own .canopy/config.json,
  // managed from the CI/CD sidebar section — not here. Kept as a separate Settings
  // section from the Project management connections on purpose.

  // Single provider today; the select exists so more CI providers slot in without
  // reshaping the form.
  type CiProvider = 'teamcity'
  const PROVIDER_OPTIONS: Array<{ value: CiProvider; label: string }> = [
    { value: 'teamcity', label: 'TeamCity' },
  ]

  let servers = $state<Array<{ baseUrl: string; username?: string }>>([])
  let editing = $state<string | null>(null) // '__new__' or the server baseUrl
  let formProvider = $state<CiProvider>('teamcity')
  let formUrl = $state('')
  let formToken = $state('')
  let testing = $state(false)
  let testResult = $state<'success' | 'fail' | ''>('')

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
    formProvider = 'teamcity'
    formUrl = ''
    formToken = ''
    testResult = ''
  }

  function startEdit(server: { baseUrl: string }): void {
    editing = server.baseUrl
    formProvider = 'teamcity'
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

  async function testConnection(): Promise<void> {
    if (!urlValid || !formToken) return
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
    if (!urlValid || !formToken) return
    const isNew = editing === '__new__'
    const encryptionAvailable = await window.api
      .isCredentialEncryptionAvailable()
      .catch(() => false)
    const storage = credentialStorageClause(window.api.platform, encryptionAvailable)
    const ok = await confirm({
      title: isNew ? 'Add CI connection' : 'Update CI connection token',
      message: `${isNew ? 'Save' : 'Update'} your TeamCity token for ${normalizedUrl}?`,
      details: `Your token is stored ${storage}, keyed by provider + URL and used by every repository that configures this CI server — never written to any repository.`,
      confirmLabel: isNew ? 'Add connection' : 'Save token',
    })
    if (!ok) return
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
  }

  async function removeServer(server: { baseUrl: string }): Promise<void> {
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
        {@render serverForm(false)}
      {:else}
        <div class="flex items-center gap-1">
          <button
            type="button"
            class="flex-1 flex items-center gap-2 px-2.5 py-1.5 border border-border-subtle rounded-md bg-bg-input text-text text-sm font-inherit cursor-pointer text-left hover:border-border min-w-0"
            onclick={() => startEdit(server)}
            title="Update the stored token for this server"
          >
            <span class="inline-flex items-center shrink-0 text-text-muted" title="TeamCity">
              <ServerCog size={14} />
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
      {@render serverForm(true)}
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

{#snippet serverForm(isNew: boolean)}
  <div class="flex flex-col gap-2 p-3 border border-border rounded-md bg-bg-input">
    <div class="flex flex-col gap-1">
      <span class="text-2xs font-semibold uppercase tracking-caps-tight text-text-faint"
        >Provider</span
      >
      {#if isNew}
        <CustomSelect
          value={formProvider}
          options={PROVIDER_OPTIONS}
          onchange={(v) => (formProvider = v as CiProvider)}
        />
      {:else}
        <span class="inline-flex items-center gap-1.5 px-2.5 py-1.5 text-sm text-text-secondary">
          <ServerCog size={14} />
          TeamCity
        </span>
      {/if}
    </div>

    <div class="flex flex-col gap-1">
      <span class="text-2xs font-semibold uppercase tracking-caps-tight text-text-faint"
        >Server URL</span
      >
      {#if isNew}
        <input
          class="px-2.5 py-1.5 border border-border rounded-md bg-bg text-text text-sm font-inherit outline-none focus:border-focus-ring placeholder:text-text-faint"
          name="ciConnectionUrl"
          aria-label="CI server URL"
          bind:value={formUrl}
          placeholder="https://teamcity.example.com"
          spellcheck="false"
        />
      {:else}
        <span class="px-2.5 py-1.5 text-sm text-text-secondary truncate" title={formUrl}
          >{formUrl}</span
        >
      {/if}
    </div>

    <div class="flex flex-col gap-2 pt-2 border-t border-border-subtle">
      <div class="flex flex-col gap-1">
        <div class="flex items-center justify-between gap-2">
          <span class="text-2xs font-semibold uppercase tracking-caps-tight text-text-faint"
            >Access token</span
          >
          <button
            type="button"
            class="text-2xs text-accent-text bg-transparent border-0 p-0 cursor-pointer underline underline-offset-2 hover:text-accent disabled:opacity-50 disabled:cursor-default disabled:no-underline"
            onclick={openTokenPage}
            disabled={!urlValid}
            title={urlValid ? 'Open the token page on this server' : 'Enter the server URL first'}
          >
            Generate →
          </button>
        </div>
        <input
          class="px-2.5 py-1.5 border border-border rounded-md bg-bg text-text text-sm font-inherit outline-none focus:border-focus-ring placeholder:text-text-faint"
          type="password"
          name="ciConnectionToken"
          aria-label="Access token"
          bind:value={formToken}
          placeholder={isNew ? 'Enter token' : '••••••••'}
          autocomplete="off"
          title="Stored encrypted on your machine, keyed by provider + URL — never written to your repository"
        />
      </div>
    </div>

    <div class="min-h-4.5" aria-live="polite">
      {#if testResult === 'success'}
        <span class="flex items-center gap-1 text-xs text-success"><Check size={13} /> OK</span>
      {:else if testResult === 'fail'}
        <span class="flex items-center gap-1 text-xs text-danger-text"><X size={13} /> Failed</span>
      {/if}
    </div>

    <div class="flex gap-1.5 justify-end">
      <button
        type="button"
        class="px-3 py-1 rounded-md text-sm font-inherit cursor-pointer border border-border bg-transparent text-text-secondary hover:bg-hover hover:text-text"
        onclick={cancelEdit}>Cancel</button
      >
      <button
        type="button"
        class="px-3 py-1 rounded-md text-sm font-inherit cursor-pointer border border-border bg-bg text-text-secondary enabled:hover:bg-hover-strong enabled:hover:text-text disabled:opacity-50 disabled:cursor-default"
        onclick={testConnection}
        disabled={testing || !urlValid || !formToken}
        title="Check the connection against the server — nothing is saved"
      >
        {testing ? 'Testing…' : 'Test'}
      </button>
      <button
        type="button"
        class="px-3 py-1 rounded-md text-sm font-inherit cursor-pointer border-0 bg-accent-bg text-accent-text enabled:hover:bg-accent-bg-hover disabled:opacity-50 disabled:cursor-default"
        onclick={saveServer}
        disabled={!urlValid || !formToken}
        title="Save the token (stored globally on this machine, per provider + URL)"
        >{isNew ? 'Add connection' : 'Save token'}</button
      >
    </div>
  </div>
{/snippet}
