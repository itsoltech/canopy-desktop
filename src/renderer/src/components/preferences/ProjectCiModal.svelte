<script lang="ts">
  import { onMount } from 'svelte'
  import { Check, LoaderCircle, Trash2, X } from '@lucide/svelte'
  import { SvelteMap, SvelteSet } from 'svelte/reactivity'
  import { closeDialog, confirm } from '../../lib/stores/dialogs.svelte'
  import { workspaceState } from '../../lib/stores/workspace.svelte'
  import { addToast } from '../../lib/stores/toast.svelte'
  import { loadCiRepoConfig } from '../../lib/stores/ci.svelte'
  import { cycleFocus } from '../../lib/a11y/focusTrap'
  import CustomSelect from '../shared/CustomSelect.svelte'
  import CustomCheckbox from '../shared/CustomCheckbox.svelte'
  import CredentialStorageNote from './_partials/CredentialStorageNote.svelte'
  import { credentialStorageClause } from './_partials/credentialStorage'

  // Per-repo CI/CD configuration (TeamCity) for the ACTIVE worktree — the analogue of
  // the Project tracker modal. The server + selected build configurations are written
  // to the git-tracked .canopy/config.json (team-shared); tokens stay personal and
  // are managed in Settings → CI connections.

  const NEW_SERVER = '__new__'

  interface ServerBuildType {
    id: string
    name: string
    projectName: string
  }

  let repoRoot = $derived(workspaceState.selectedWorktreePath ?? workspaceState.repoRoot)
  let containerEl: HTMLElement | undefined = $state()

  let existingConfig = $state<{
    baseUrl: string
    buildTypes: Array<{ id: string; label: string }>
  } | null>(null)
  let servers = $state<Array<{ baseUrl: string }>>([])
  let selectedServer = $state<string>(NEW_SERVER)
  let newUrl = $state('')
  let formToken = $state('')
  let testing = $state(false)
  let testResult = $state<'success' | 'fail' | ''>('')
  let saving = $state(false)

  let serverTypes = $state<ServerBuildType[]>([])
  let typesLoading = $state(false)
  let typesError = $state('')
  let typesLoaded = $state(false)
  const selected = new SvelteMap<string, string>()
  // URLs whose existing-config selection was already seeded once — an empty `selected`
  // is ALSO what a user who deselected everything has, so re-seeding must not key on size.
  const seededFor = new SvelteSet<string>()

  let effectiveUrl = $derived(
    selectedServer === NEW_SERVER ? newUrl.trim().replace(/\/$/, '') : selectedServer,
  )
  let urlValid = $derived(/^https?:\/\/\S+$/i.test(effectiveUrl))
  let serverHasToken = $derived(servers.some((s) => s.baseUrl === selectedServer))
  let canLoadTypes = $derived(urlValid && (serverHasToken || formToken.length > 0))

  let serverOptions = $derived.by(() => {
    const options = servers.map((s) => ({ value: s.baseUrl, label: s.baseUrl }))
    // An edited config may point at a server with no stored token — keep it pickable.
    if (existingConfig && !servers.some((s) => s.baseUrl === existingConfig!.baseUrl)) {
      options.push({
        value: existingConfig.baseUrl,
        label: `${existingConfig.baseUrl} (no token)`,
      })
    }
    options.push({ value: NEW_SERVER, label: 'Add new server…' })
    return options
  })

  onMount(async () => {
    containerEl?.focus()
    try {
      const all = await window.api.keychainListCredentials()
      servers = all.filter((c) => c.provider === 'teamcity').map((c) => ({ baseUrl: c.baseUrl }))
    } catch {
      servers = []
    }
    if (repoRoot) {
      try {
        existingConfig = await window.api.ciConfig(repoRoot)
      } catch {
        existingConfig = null
      }
    }
    if (existingConfig) {
      selectedServer = existingConfig.baseUrl
      seededFor.add(existingConfig.baseUrl)
      for (const bt of existingConfig.buildTypes) selected.set(bt.id, bt.label)
      // Editing with a stored token: show the picker right away.
      if (servers.some((s) => s.baseUrl === existingConfig!.baseUrl)) void loadBuildTypes()
    } else if (servers.length > 0) {
      selectedServer = servers[0].baseUrl
    }
  })

  function handleKeydown(e: KeyboardEvent): void {
    if (e.key === 'Escape') {
      e.preventDefault()
      closeDialog()
      return
    }
    if (e.key === 'Tab' && containerEl) cycleFocus(containerEl, e)
  }

  function selectServer(value: string): void {
    selectedServer = value
    testResult = ''
    typesLoaded = false
    serverTypes = []
    typesError = ''
    // The selection belongs to the server it was loaded from — carrying it across
    // would let Save write these ids under a different baseUrl into the git-shared
    // config, where nothing cross-checks that the jobs exist on that server.
    selected.clear()
    if (value !== NEW_SERVER && servers.some((s) => s.baseUrl === value)) void loadBuildTypes()
  }

  // The one URL the user has explicitly acknowledged sending the token to. Tracking
  // the URL itself (not a flag) means switching servers or editing the address
  // automatically re-asks, while Test → Load available jobs prompts only once.
  let acknowledgedUrl = $state('')

  /**
   * Destination gate for every path that sends the typed token: `effectiveUrl` can
   * come from the git-SHARED repo config (editing an existing setup preselects it),
   * so the token must never leave the machine unacknowledged.
   */
  async function confirmDestination(): Promise<boolean> {
    if (effectiveUrl === acknowledgedUrl) return true
    const encryptionAvailable = await window.api
      .isCredentialEncryptionAvailable()
      .catch(() => false)
    const storage = credentialStorageClause(window.api.platform, encryptionAvailable)
    const insecure = effectiveUrl.startsWith('http://')
    const ok = await confirm({
      title: 'Confirm CI server address',
      message: `Send your TeamCity token to ${effectiveUrl}?`,
      details:
        `The token will be sent only to this address and, when saved, stored ${storage}, keyed by provider + URL. Only continue if you recognize it as your TeamCity server.` +
        (insecure
          ? ' Warning: this is a plain http:// address — the token would travel unencrypted.'
          : ''),
      confirmLabel: 'Continue',
    })
    if (ok) acknowledgedUrl = effectiveUrl
    return ok
  }

  async function testConnection(): Promise<void> {
    if (!urlValid || !formToken) return
    if (!(await confirmDestination())) return
    testing = true
    testResult = ''
    try {
      await window.api.ciTestNewConnection(effectiveUrl, formToken)
      testResult = 'success'
    } catch {
      testResult = 'fail'
    } finally {
      testing = false
    }
  }

  /** Stores a typed token (behind the destination gate) before first use. */
  async function ensureToken(): Promise<boolean> {
    if (serverHasToken && !formToken) return true
    if (!formToken) return false
    if (!(await confirmDestination())) return false
    try {
      await window.api.keychainSetCredentials('teamcity', effectiveUrl, formToken)
    } catch (e) {
      addToast(e instanceof Error ? e.message : 'Failed to save credentials')
      return false
    }
    if (!servers.some((s) => s.baseUrl === effectiveUrl)) {
      servers = [...servers, { baseUrl: effectiveUrl }]
    }
    if (selectedServer === NEW_SERVER) selectedServer = effectiveUrl
    formToken = ''
    return true
  }

  async function loadBuildTypes(): Promise<void> {
    if (!urlValid) return
    if (!(await ensureToken())) return
    typesLoading = true
    typesError = ''
    try {
      serverTypes = await window.api.ciListBuildTypes(effectiveUrl)
      typesLoaded = true
      // Returning to the server the repo is already configured against re-ticks its
      // jobs ONCE (selectServer cleared them — the selection is per-server). Tracked
      // by a flag, not by `selected.size`: an empty map is also what a user who
      // deselected everything has, and their choice must not be undone on reload.
      if (existingConfig && effectiveUrl === existingConfig.baseUrl && !seededFor.has(effectiveUrl)) {
        seededFor.add(effectiveUrl)
        for (const bt of existingConfig.buildTypes) selected.set(bt.id, bt.label)
      }
    } catch (e) {
      typesError = e instanceof Error ? e.message : 'Failed to load build configurations'
      serverTypes = []
    } finally {
      typesLoading = false
    }
  }

  let groupedTypes = $derived.by(() => {
    const groups = new SvelteMap<string, ServerBuildType[]>()
    for (const bt of serverTypes) {
      const key = bt.projectName || 'Other'
      const list = groups.get(key)
      if (list) list.push(bt)
      else groups.set(key, [bt])
    }
    return [...groups.entries()]
  })

  function toggleType(bt: ServerBuildType): void {
    if (selected.has(bt.id)) selected.delete(bt.id)
    else selected.set(bt.id, selected.get(bt.id) ?? bt.name)
  }

  async function saveConfiguration(): Promise<void> {
    if (!repoRoot || selected.size === 0 || !urlValid) return
    saving = true
    try {
      await window.api.ciSaveConfig(repoRoot, {
        baseUrl: effectiveUrl,
        buildTypes: [...selected.entries()].map(([id, label]) => ({ id, label })),
      })
      await loadCiRepoConfig(repoRoot)
      addToast('CI configuration saved — commit .canopy/config.json to share it')
      closeDialog()
    } catch (e) {
      addToast(e instanceof Error ? e.message : 'Failed to save CI configuration')
    } finally {
      saving = false
    }
  }

  async function removeConfiguration(): Promise<void> {
    if (!repoRoot || !existingConfig) return
    const ok = await confirm({
      title: 'Remove CI configuration',
      message: `Remove the TeamCity configuration (${existingConfig.baseUrl}) from this repository?`,
      details:
        'Removes the ci block from the git-tracked .canopy/config.json — after committing, the whole team loses the CI rows. Your stored token stays (Settings → CI connections).',
      confirmLabel: 'Remove configuration',
      destructive: true,
    })
    if (!ok) return
    try {
      await window.api.ciSaveConfig(repoRoot, null)
      await loadCiRepoConfig(repoRoot)
      addToast('CI configuration removed')
      closeDialog()
    } catch (e) {
      addToast(e instanceof Error ? e.message : 'Failed to remove CI configuration')
    }
  }
</script>

<!-- svelte-ignore a11y_no_static_element_interactions -->
<div
  class="fixed inset-0 z-overlay flex justify-center items-center bg-scrim"
  onkeydown={handleKeydown}
  onmousedown={closeDialog}
>
  <!-- svelte-ignore a11y_click_events_have_key_events -->
  <div
    bind:this={containerEl}
    class="outline-none w-[620px] max-w-[92vw] max-h-[85vh] flex flex-col bg-bg-overlay border border-border rounded-xl shadow-modal overflow-hidden"
    role="dialog"
    aria-modal="true"
    aria-label="CI/CD configuration"
    tabindex="-1"
    onmousedown={(e) => e.stopPropagation()}
  >
    <header
      class="px-6 pt-5 pb-3 border-b border-border-subtle shrink-0 flex items-start justify-between gap-3"
    >
      <div class="flex flex-col gap-0.5 min-w-0">
        <h2 class="text-lg font-semibold text-text m-0 leading-tight">CI/CD — TeamCity</h2>
        <p class="text-xs text-text-muted m-0 leading-snug">
          The server and the available build configurations are shared with your team via
          <code class="font-mono">.canopy/config.json</code> in this repository. Tokens stay on this machine.
        </p>
      </div>
      <button
        type="button"
        class="flex items-center justify-center size-7 rounded-md bg-transparent border-0 text-text-muted cursor-pointer hover:bg-hover hover:text-text shrink-0"
        onclick={closeDialog}
        aria-label="Close"
        title="Close"
      >
        <X size={16} />
      </button>
    </header>

    <div class="flex-1 overflow-y-auto px-6 py-5 flex flex-col gap-4">
      {#if !repoRoot}
        <p class="text-sm text-text-faint m-0">Open a repository first.</p>
      {:else}
        <div class="flex flex-col gap-1">
          <span class="text-2xs font-semibold uppercase tracking-caps-tight text-text-faint"
            >Server</span
          >
          <CustomSelect value={selectedServer} options={serverOptions} onchange={selectServer} />
        </div>

        {#if selectedServer === NEW_SERVER}
          <div class="flex flex-col gap-1">
            <span class="text-2xs font-semibold uppercase tracking-caps-tight text-text-faint"
              >Server URL</span
            >
            <input
              class="px-2.5 py-1.5 border border-border rounded-md bg-bg-input text-text text-sm font-inherit outline-none focus:border-focus-ring placeholder:text-text-faint"
              name="ciModalUrl"
              aria-label="TeamCity server URL"
              bind:value={newUrl}
              placeholder="https://teamcity.example.com"
              spellcheck="false"
            />
          </div>
        {/if}

        {#if !serverHasToken || selectedServer === NEW_SERVER}
          <div class="flex flex-col gap-1">
            <span class="text-2xs font-semibold uppercase tracking-caps-tight text-text-faint"
              >Access token</span
            >
            <input
              class="px-2.5 py-1.5 border border-border rounded-md bg-bg-input text-text text-sm font-inherit outline-none focus:border-focus-ring placeholder:text-text-faint"
              type="password"
              name="ciModalToken"
              aria-label="TeamCity access token"
              bind:value={formToken}
              placeholder="Enter token"
              autocomplete="off"
              title="Stored encrypted on your machine, keyed by provider + URL — never written to your repository"
            />
            <div class="mt-1">
              <CredentialStorageNote
                provider="teamcity"
                baseUrl={urlValid ? effectiveUrl : undefined}
                sharingNote={false}
              />
            </div>
          </div>
        {/if}

        <div class="flex items-center gap-1.5">
          {#if formToken}
            <button
              type="button"
              class="px-3 py-1 rounded-md text-sm font-inherit cursor-pointer border border-border bg-bg-input text-text-secondary enabled:hover:bg-hover-strong enabled:hover:text-text disabled:opacity-50 disabled:cursor-default"
              onclick={testConnection}
              disabled={testing || !urlValid}
              title="Check the connection against the server — nothing is saved"
            >
              {testing ? 'Testing…' : 'Test'}
            </button>
          {/if}
          <button
            type="button"
            class="px-3 py-1 rounded-md text-sm font-inherit cursor-pointer border border-border bg-bg-input text-text-secondary enabled:hover:bg-hover-strong enabled:hover:text-text disabled:opacity-50 disabled:cursor-default"
            onclick={loadBuildTypes}
            disabled={typesLoading || !canLoadTypes}
            title="Saves the token (when entered) and fetches the list of jobs (build configurations) from the TeamCity server"
          >
            {typesLoading ? 'Loading…' : 'Load available jobs'}
          </button>
          <span class="min-w-4" aria-live="polite">
            {#if testResult === 'success'}
              <span class="flex items-center gap-1 text-xs text-success"
                ><Check size={13} /> OK</span
              >
            {:else if testResult === 'fail'}
              <span class="flex items-center gap-1 text-xs text-danger-text"
                ><X size={13} /> Failed</span
              >
            {/if}
          </span>
        </div>

        {#if typesError}
          <span class="text-xs text-danger-text">{typesError}</span>
        {/if}

        {#if typesLoading && !typesLoaded}
          <div class="flex items-center gap-2 text-sm text-text-faint">
            <LoaderCircle size={13} class="animate-spin-slow motion-reduce:animate-none" />
            Loading available jobs…
          </div>
        {:else if typesLoaded}
          {#if serverTypes.length === 0}
            <span class="text-xs text-text-faint">The server exposes no jobs.</span>
          {:else}
            <div class="flex flex-col gap-2">
              <p class="m-0 text-xs text-text-muted leading-snug">
                These are all the jobs (build configurations) the TeamCity server exposes. Check the
                ones that belong to THIS repository — only those appear in Canopy (the CI/CD
                section, Run job and the branch context menu). The selection is written to the
                git-tracked <code class="font-mono">.canopy/config.json</code>, so after you commit
                it the whole team gets the same jobs. Labels are editable and shown in the sidebar.
              </p>
              {#each groupedTypes as [project, types] (project)}
                <div class="flex flex-col gap-1">
                  <span class="text-2xs font-semibold uppercase tracking-caps-tight text-text-faint"
                    >{project}</span
                  >
                  {#each types as bt (bt.id)}
                    <div class="flex items-center gap-2">
                      <label
                        class="flex items-center gap-2 text-sm text-text-secondary cursor-pointer select-none min-w-0"
                      >
                        <CustomCheckbox
                          checked={selected.has(bt.id)}
                          onchange={() => toggleType(bt)}
                        />
                        <span class="truncate" title={bt.id}>{bt.name}</span>
                      </label>
                      {#if selected.has(bt.id)}
                        <input
                          class="flex-1 min-w-24 max-w-48 px-2 py-0.5 border border-border rounded-md bg-bg-input text-text text-xs font-inherit outline-none focus:border-focus-ring"
                          aria-label={`Sidebar label for ${bt.name}`}
                          value={selected.get(bt.id) ?? bt.name}
                          oninput={(e) => selected.set(bt.id, e.currentTarget.value)}
                          title="Label shown in the sidebar"
                        />
                      {/if}
                    </div>
                  {/each}
                </div>
              {/each}
            </div>
          {/if}
        {/if}
      {/if}
    </div>

    <footer
      class="px-6 py-3 border-t border-border-subtle shrink-0 flex items-center justify-between gap-2"
    >
      <div>
        {#if existingConfig}
          <button
            type="button"
            class="flex items-center gap-1 px-2 py-1 rounded-md border-0 bg-transparent text-text-faint text-xs font-inherit cursor-pointer hover:text-danger-text"
            onclick={removeConfiguration}
          >
            <Trash2 size={12} />
            Remove CI configuration
          </button>
        {/if}
      </div>
      <div class="flex items-center gap-1.5">
        <button
          type="button"
          class="px-3 py-1 rounded-md text-sm font-inherit cursor-pointer border border-border bg-transparent text-text-secondary hover:bg-hover hover:text-text"
          onclick={closeDialog}>Cancel</button
        >
        <button
          type="button"
          class="px-3 py-1 rounded-md text-sm font-inherit cursor-pointer border-0 bg-accent-bg text-accent-text enabled:hover:bg-accent-bg-hover disabled:opacity-50 disabled:cursor-default"
          onclick={saveConfiguration}
          disabled={saving || !typesLoaded || selected.size === 0 || !urlValid}
          title="Writes the ci block to .canopy/config.json — commit it to share with the team"
          >{saving ? 'Saving…' : 'Save configuration'}</button
        >
      </div>
    </footer>
  </div>
</div>
