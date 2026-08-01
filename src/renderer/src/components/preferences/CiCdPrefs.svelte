<script lang="ts">
  import { Check, Pencil, ServerCog, Unlink, X } from '@lucide/svelte'
  import { SvelteMap } from 'svelte/reactivity'
  import { confirm } from '../../lib/stores/dialogs.svelte'
  import { workspaceState } from '../../lib/stores/workspace.svelte'
  import { prefs, setPref } from '../../lib/stores/preferences.svelte'
  import { addToast } from '../../lib/stores/toast.svelte'
  import CustomCheckbox from '../shared/CustomCheckbox.svelte'
  import PrefsSection from './_partials/PrefsSection.svelte'
  import CredentialStorageNote from './_partials/CredentialStorageNote.svelte'

  // CI/CD (TeamCity) for the ACTIVE repository. The configuration (server URL +
  // which build configurations are available) is repo-owned: it is written to the
  // git-tracked .canopy/config.json so the whole team shares it — analogous to the
  // per-repo project/board selection in Project management. Only the token (keychain,
  // per provider+URL) and the ci.enabled opt-in are personal.

  interface BuildTypeEntry {
    id: string
    label: string
  }
  interface ServerBuildType {
    id: string
    name: string
    projectName: string
  }

  let repoRoot = $derived(workspaceState.selectedWorktreePath ?? workspaceState.repoRoot)
  let ciEnabled = $derived(prefs['ci.enabled'] === 'true')

  // Validated by the main process (`ci:config`) — never read the raw repo config value.
  let ciConfig = $state<{ baseUrl: string; buildTypes: BuildTypeEntry[] } | null>(null)
  let configLoaded = $state(false)
  let hasToken = $state(false)

  async function reloadCiConfig(): Promise<void> {
    const root = repoRoot
    if (!root) {
      ciConfig = null
      configLoaded = true
      return
    }
    try {
      const cfg = await window.api.ciConfig(root)
      if (repoRoot !== root) return
      ciConfig = cfg
      hasToken = cfg ? await window.api.keychainHasCredentials('teamcity', cfg.baseUrl) : false
    } catch {
      if (repoRoot === root) ciConfig = null
    } finally {
      if (repoRoot === root) configLoaded = true
    }
  }

  $effect(() => {
    void repoRoot
    void reloadCiConfig()
  })

  // --- Configurator (init + edit share the same form) ---

  let editing = $state(false)
  let editBaseUrl = $state('')
  let editToken = $state('')
  let testing = $state(false)
  let testResult = $state<'success' | 'fail' | ''>('')
  let saving = $state(false)

  // Server-side list of build configurations + the per-repo selection (id → label).
  let serverTypes = $state<ServerBuildType[]>([])
  let typesLoading = $state(false)
  let typesError = $state('')
  let typesLoaded = $state(false)
  const selected = new SvelteMap<string, string>()

  // The URL the token will be sent to must be explicitly acknowledged once per edit
  // session — when editing an existing config it comes from the git-shared repo file.
  let destinationAcknowledged = $state(false)

  let normalizedEditUrl = $derived(editBaseUrl.trim().replace(/\/$/, ''))
  let editUrlValid = $derived(/^https?:\/\/\S+$/i.test(normalizedEditUrl))

  function startConfigure(): void {
    editing = true
    editBaseUrl = ciConfig?.baseUrl ?? ''
    editToken = ''
    testResult = ''
    destinationAcknowledged = false
    serverTypes = []
    typesError = ''
    typesLoaded = false
    selected.clear()
    for (const bt of ciConfig?.buildTypes ?? []) selected.set(bt.id, bt.label)
  }

  function cancelConfigure(): void {
    editing = false
    testResult = ''
  }

  async function ensureDestinationAcknowledged(): Promise<boolean> {
    if (destinationAcknowledged) return true
    const insecure = normalizedEditUrl.startsWith('http://')
    const ok = await confirm({
      title: 'Confirm CI server address',
      message: `Continue with the TeamCity server at ${normalizedEditUrl}?`,
      details:
        'Your token will be sent to this address. When the configuration comes from the repository, the address lives in the git-shared .canopy/config.json — only continue if you recognize it as your TeamCity server.' +
        (insecure
          ? ' Warning: this is a plain http:// address — the token would travel unencrypted.'
          : ''),
      confirmLabel: 'Continue',
    })
    if (ok) destinationAcknowledged = true
    return ok
  }

  async function testConnection(): Promise<void> {
    if (!editUrlValid || !editToken) return
    if (!(await ensureDestinationAcknowledged())) return
    testing = true
    testResult = ''
    try {
      await window.api.ciTestNewConnection(normalizedEditUrl, editToken)
      testResult = 'success'
    } catch {
      testResult = 'fail'
    } finally {
      testing = false
    }
  }

  async function openTokenPage(): Promise<void> {
    if (!editUrlValid) return
    if (!(await ensureDestinationAcknowledged())) return
    window.api.openExternal(`${normalizedEditUrl}/profile.html?item=accessTokens`)
  }

  // A URL change invalidates both the loaded list and the acknowledgment.
  function onBaseUrlInput(): void {
    typesLoaded = false
    serverTypes = []
    typesError = ''
    destinationAcknowledged = false
  }

  /** Saves the token (when one was typed) and fetches the server's build configurations. */
  async function loadBuildTypes(): Promise<void> {
    if (!editUrlValid) return
    if (!(await ensureDestinationAcknowledged())) return
    typesLoading = true
    typesError = ''
    try {
      if (editToken) {
        await window.api.keychainSetCredentials('teamcity', normalizedEditUrl, editToken)
        hasToken = true
        editToken = ''
      }
      serverTypes = await window.api.ciListBuildTypes(normalizedEditUrl)
      typesLoaded = true
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
    if (!repoRoot || selected.size === 0) return
    saving = true
    try {
      await window.api.ciSaveConfig(repoRoot, {
        baseUrl: normalizedEditUrl,
        buildTypes: [...selected.entries()].map(([id, label]) => ({ id, label })),
      })
      // Configuring CI here IS the personal opt-in — flip the (default-off) flag so
      // the section appears without a second trip through Settings.
      if (!ciEnabled) setPref('ci.enabled', 'true')
      editing = false
      await reloadCiConfig()
      addToast('CI configuration saved — commit .canopy/config.json to share it')
    } catch (e) {
      addToast(e instanceof Error ? e.message : 'Failed to save CI configuration')
    } finally {
      saving = false
    }
  }

  async function removeConfiguration(): Promise<void> {
    if (!repoRoot || !ciConfig) return
    const ok = await confirm({
      title: 'Remove CI configuration',
      message: `Remove the TeamCity configuration (${ciConfig.baseUrl}) from this repository?`,
      details:
        'Removes the ci block from the git-tracked .canopy/config.json — after committing, the whole team loses the CI rows. Your stored token stays and can be removed separately.',
      confirmLabel: 'Remove configuration',
      destructive: true,
    })
    if (!ok) return
    try {
      await window.api.ciSaveConfig(repoRoot, null)
      await reloadCiConfig()
      addToast('CI configuration removed')
    } catch (e) {
      addToast(e instanceof Error ? e.message : 'Failed to remove CI configuration')
    }
  }

  async function removeCredentials(): Promise<void> {
    if (!ciConfig) return
    const ok = await confirm({
      title: 'Remove credentials',
      message: `Remove your stored token for TeamCity at ${ciConfig.baseUrl}?`,
      details:
        'Clears the token on this machine only. The CI configuration stays in the repository.',
      confirmLabel: 'Remove credentials',
      destructive: true,
    })
    if (!ok) return
    try {
      await window.api.keychainDeleteCredentials('teamcity', ciConfig.baseUrl)
    } catch (e) {
      addToast(e instanceof Error ? e.message : 'Failed to remove credentials')
      return
    }
    hasToken = false
    addToast('Credentials removed')
  }
</script>

<PrefsSection
  title="TeamCity"
  description="Build status and triggering for this project — the server and the available build configurations are shared with the team via .canopy/config.json; the token and the toggle below are yours"
>
  <div class="flex flex-col gap-2">
    {#if !repoRoot}
      <p class="text-sm text-text-faint m-0">Open a repository to configure its CI/CD.</p>
    {:else}
      <!-- Personal opt-in (default off): a git-shared ci block must not enable UI and
           polling for everyone opening the repo. -->
      <label class="flex items-center gap-2 text-sm text-text-secondary cursor-pointer select-none">
        <CustomCheckbox
          checked={ciEnabled}
          onchange={() => setPref('ci.enabled', ciEnabled ? 'false' : 'true')}
        />
        <span>Show CI build status in the sidebar GIT section</span>
      </label>

      {#if !configLoaded}
        <p class="text-sm text-text-faint m-0">Loading…</p>
      {:else if !ciConfig && !editing}
        <p class="text-sm text-text-faint m-0">
          No CI is configured in this project's
          <code class="font-mono text-text-secondary">.canopy/config.json</code>.
        </p>
        <button
          type="button"
          class="self-start flex items-center gap-1.5 px-3 py-1 rounded-md bg-accent-bg border-0 text-accent-text text-sm font-inherit cursor-pointer hover:bg-accent-bg-hover"
          onclick={startConfigure}
        >
          <ServerCog size={13} />
          Configure TeamCity
        </button>
      {:else if ciConfig && !editing}
        <div class="flex items-center gap-1">
          <div
            class="flex-1 flex items-center gap-2 px-2.5 py-1.5 border border-border-subtle rounded-md bg-bg-input text-text text-sm min-w-0"
          >
            <span class="inline-flex items-center shrink-0 text-text-muted" title="TeamCity">
              <ServerCog size={14} />
            </span>
            <span class="flex-1 text-text-secondary truncate" title={ciConfig.baseUrl}
              >{ciConfig.baseUrl}</span
            >
            {#if hasToken}
              <span
                class="flex items-center gap-1 text-2xs text-success shrink-0"
                title="Credentials saved"
              >
                <Check size={12} />
              </span>
            {:else}
              <span class="text-2xs text-warning-text shrink-0">No credentials</span>
            {/if}
          </div>
          <button
            type="button"
            class="flex items-center justify-center size-7 rounded-md bg-transparent border-0 text-text-muted cursor-pointer hover:bg-hover hover:text-text"
            onclick={startConfigure}
            aria-label="Edit CI configuration"
            title="Edit the server, token and available build configurations"
          >
            <Pencil size={12} />
          </button>
          {#if hasToken}
            <button
              type="button"
              class="flex items-center justify-center size-7 rounded-md bg-transparent border-0 text-text-muted cursor-pointer hover:bg-hover hover:text-text"
              onclick={removeCredentials}
              aria-label="Remove TeamCity credentials"
              title={`Remove the stored token for TeamCity at ${ciConfig.baseUrl}`}
            >
              <Unlink size={12} />
            </button>
          {/if}
        </div>

        <div class="flex flex-col gap-1 mx-1 px-3 py-2 rounded-md border border-border-subtle">
          <span class="text-2xs font-semibold uppercase tracking-caps-tight text-text-faint"
            >Available build configurations</span
          >
          {#each ciConfig.buildTypes as bt (bt.id)}
            <div class="flex items-center gap-2 text-sm">
              <span class="text-text-secondary">{bt.label}</span>
              {#if bt.label !== bt.id}
                <span class="font-mono text-xs text-text-faint truncate" title={bt.id}>{bt.id}</span
                >
              {/if}
            </div>
          {/each}
        </div>

        <button
          type="button"
          class="self-start px-1.5 py-0.5 border-0 bg-transparent text-text-faint text-xs font-inherit cursor-pointer hover:text-danger-text"
          onclick={removeConfiguration}
        >
          Remove CI configuration from this repository
        </button>
      {/if}

      {#if editing}
        <div class="flex flex-col gap-2 p-3 border border-border rounded-md bg-bg-input">
          <div class="flex flex-col gap-1">
            <span class="text-2xs font-semibold uppercase tracking-caps-tight text-text-faint"
              >Server URL</span
            >
            <input
              class="px-2.5 py-1.5 border border-border rounded-md bg-bg text-text text-sm font-inherit outline-none focus:border-focus-ring placeholder:text-text-faint"
              name="ciBaseUrl"
              aria-label="TeamCity server URL"
              bind:value={editBaseUrl}
              oninput={onBaseUrlInput}
              placeholder="https://teamcity.example.com"
              spellcheck="false"
            />
          </div>

          <div class="flex flex-col gap-1">
            <div class="flex items-center justify-between gap-2">
              <span class="text-2xs font-semibold uppercase tracking-caps-tight text-text-faint"
                >Access token</span
              >
              <button
                type="button"
                class="text-2xs text-accent-text bg-transparent border-0 p-0 cursor-pointer underline underline-offset-2 hover:text-accent disabled:opacity-50 disabled:cursor-default"
                onclick={openTokenPage}
                disabled={!editUrlValid}
              >
                Generate →
              </button>
            </div>
            <input
              class="px-2.5 py-1.5 border border-border rounded-md bg-bg text-text text-sm font-inherit outline-none focus:border-focus-ring placeholder:text-text-faint"
              type="password"
              name="ciToken"
              aria-label="TeamCity access token"
              bind:value={editToken}
              placeholder={hasToken ? '•••••••• (stored — leave empty to keep)' : 'Enter token'}
              autocomplete="off"
              title="Stored encrypted on your machine, keyed by provider + URL — never written to your repository"
            />
          </div>

          <div class="flex items-center gap-1.5">
            <button
              type="button"
              class="px-3 py-1 rounded-md text-sm font-inherit cursor-pointer border border-border bg-bg text-text-secondary enabled:hover:bg-hover-strong enabled:hover:text-text disabled:opacity-50 disabled:cursor-default"
              onclick={testConnection}
              disabled={testing || !editUrlValid || !editToken}
              title="Check the connection against the server — nothing is saved"
            >
              {testing ? 'Testing…' : 'Test'}
            </button>
            <button
              type="button"
              class="px-3 py-1 rounded-md text-sm font-inherit cursor-pointer border border-border bg-bg text-text-secondary enabled:hover:bg-hover-strong enabled:hover:text-text disabled:opacity-50 disabled:cursor-default"
              onclick={loadBuildTypes}
              disabled={typesLoading || !editUrlValid || (!editToken && !hasToken)}
              title="Saves the token (when entered) and lists the server's build configurations"
            >
              {typesLoading ? 'Loading…' : 'Load build configurations'}
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

          {#if typesLoaded}
            {#if serverTypes.length === 0}
              <span class="text-xs text-text-faint"
                >The server exposes no build configurations.</span
              >
            {:else}
              <div class="flex flex-col gap-2 max-h-72 overflow-y-auto pr-1">
                <p class="m-0 text-xs text-text-muted leading-snug">
                  Build configurations available in this repository — the selection is saved to the
                  git-tracked config, so it applies to the whole team. Labels are shown in the
                  sidebar.
                </p>
                {#each groupedTypes as [project, types] (project)}
                  <div class="flex flex-col gap-1">
                    <span
                      class="text-2xs font-semibold uppercase tracking-caps-tight text-text-faint"
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
                            class="flex-1 min-w-24 max-w-48 px-2 py-0.5 border border-border rounded-md bg-bg text-text text-xs font-inherit outline-none focus:border-focus-ring"
                            aria-label={`Sidebar label for ${bt.name}`}
                            value={selected.get(bt.id) ?? bt.name}
                            oninput={(e) => selected.set(bt.id, e.currentTarget.value)}
                            title="Label shown in the sidebar GIT section"
                          />
                        {/if}
                      </div>
                    {/each}
                  </div>
                {/each}
              </div>
            {/if}
          {/if}

          <div class="flex gap-1.5 justify-end pt-1 border-t border-border-subtle">
            <button
              type="button"
              class="px-3 py-1 rounded-md text-sm font-inherit cursor-pointer border border-border bg-transparent text-text-secondary hover:bg-hover hover:text-text"
              onclick={cancelConfigure}>Cancel</button
            >
            <button
              type="button"
              class="px-3 py-1 rounded-md text-sm font-inherit cursor-pointer border-0 bg-accent-bg text-accent-text enabled:hover:bg-accent-bg-hover disabled:opacity-50 disabled:cursor-default"
              onclick={saveConfiguration}
              disabled={saving || selected.size === 0 || !editUrlValid}
              title="Writes the ci block to .canopy/config.json — commit it to share with the team"
              >{saving ? 'Saving…' : 'Save configuration'}</button
            >
          </div>

          <CredentialStorageNote
            provider="teamcity"
            baseUrl={editUrlValid ? normalizedEditUrl : undefined}
            sharingNote={false}
          />
        </div>
      {/if}
    {/if}
  </div>
</PrefsSection>
