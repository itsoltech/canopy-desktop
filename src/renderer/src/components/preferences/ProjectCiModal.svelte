<script lang="ts">
  import { onMount, tick } from 'svelte'
  import { Check, LoaderCircle, Trash2, X } from '@lucide/svelte'
  import { SvelteMap } from 'svelte/reactivity'
  import { closeDialog, confirm } from '../../lib/stores/dialogs.svelte'
  import { workspaceState } from '../../lib/stores/workspace.svelte'
  import { addToast } from '../../lib/stores/toast.svelte'
  import { bumpCiCredentialTick, loadCiRepoConfig } from '../../lib/stores/ci.svelte'
  import { cycleFocus } from '../../lib/a11y/focusTrap'
  import { canUseTeamCityCredential, teamCityCredentialGate } from '../../lib/ci/credentialGate'
  import { ipcErrorMessage } from '../../lib/ci/errors'
  import { CI_MAX_BUILD_TYPES } from '../../lib/ci/limits'
  import type { CiCredentialStatus, TeamCityCiRepoConfigInfo } from '../../lib/ci/types'
  import CustomSelect from '../shared/CustomSelect.svelte'
  import CiJobPicker from '../ci/CiJobPicker.svelte'
  import CredentialStorageNote from './_partials/CredentialStorageNote.svelte'
  import CiCredentialModal from './CiCredentialModal.svelte'
  import { credentialStorageClause } from './_partials/credentialStorage'

  interface InvalidCiConfig {
    scope: 'file' | 'block'
    message: string
    provider?: 'teamcity' | 'github-actions'
  }

  let {
    initialConfig,
    initialCredential,
    initialInvalid,
  }: {
    initialConfig: TeamCityCiRepoConfigInfo | null
    initialCredential?: CiCredentialStatus
    initialInvalid?: InvalidCiConfig
  } = $props()

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

  // The SHARED shape (renderer mirror of the preload CiConfigInfo) — an inline
  // copy already drifted once, reading a field the copy didn't declare.
  let existingConfig = $state<TeamCityCiRepoConfigInfo | null>(null)
  /** Set when a ci block exists but could not be used — shown so the advertised
      fix-and-re-save path names what is wrong. The scope picks the ONE recovery
      route to offer (file → hand-edit, Save disabled; block → re-save replaces). */
  let configLoadError = $state('')
  let configLoadScope = $state<'file' | 'block' | ''>('')
  /** Save/remove failure — surfaced in the footer: a toast would render UNDER
      this modal's scrim (z-banner 9999 < z-overlay 10000) and be unclickable. */
  let saveError = $state('')
  let servers = $state<
    Array<{
      baseUrl: string
      authenticationState: CiCredentialStatus['authenticationState']
    }>
  >([])
  let configuredCredentialOverride = $state<CiCredentialStatus | null>(null)
  let selectedServer = $state<string>(NEW_SERVER)
  let newUrl = $state('')
  let formToken = $state('')
  let trimmedFormToken = $derived(formToken.trim())
  let testing = $state(false)
  let testResult = $state<'success' | 'fail' | ''>('')
  let saving = $state(false)
  // VISIBLE busy state, per action — set only after a confirm resolves, so
  // "Saving…" never shows (and Remove never dims/blurs itself) while the user is
  // deciding, and each button renders only ITS OWN spinner: one shared flag made
  // a Save animate Remove's icon, announcing a removal that wasn't happening.
  // `saving` stays the pre-confirm re-entrancy guard shared by both actions.
  let busy = $state<'' | 'save' | 'remove'>('')
  let credentialEditorOpen = $state(false)
  let credentialButtonEl: HTMLButtonElement | undefined = $state()

  let serverTypes = $state<ServerBuildType[]>([])
  let typesLoading = $state(false)
  let typesError = $state('')
  let typesLoaded = $state(false)
  const selected = new SvelteMap<string, string>()

  let effectiveUrl = $derived(
    selectedServer === NEW_SERVER ? newUrl.trim().replace(/\/$/, '') : selectedServer,
  )
  let urlValid = $derived(/^https?:\/\/\S+$/i.test(effectiveUrl))
  let selectedCredential = $derived.by((): CiCredentialStatus => {
    if (existingConfig && selectedServer === existingConfig.baseUrl) {
      return (
        configuredCredentialOverride ??
        initialCredential ?? { hasToken: false, authenticationState: 'unknown' }
      )
    }
    const stored = servers.find((server) => server.baseUrl === selectedServer)
    return {
      hasToken: !!stored,
      authenticationState: stored?.authenticationState ?? 'unknown',
    }
  })
  let credentialGate = $derived(teamCityCredentialGate(selectedCredential))
  let serverHasToken = $derived(selectedCredential.hasToken)
  let credentialRejected = $derived(
    selectedCredential.hasToken && selectedCredential.authenticationState === 'invalid',
  )
  let isInitialSetup = $derived(initialConfig === null)
  let canLoadTypes = $derived(
    urlValid && canUseTeamCityCredential(selectedCredential, trimmedFormToken.length > 0),
  )

  let serverOptions = $derived.by(() => {
    const options = servers.map((s) => ({ value: s.baseUrl, label: s.baseUrl }))
    // An edited config may point at a server with no stored token — keep it pickable.
    if (existingConfig && !servers.some((s) => s.baseUrl === existingConfig!.baseUrl)) {
      options.push({
        value: existingConfig.baseUrl,
        label: `${existingConfig.baseUrl} (no token)`,
      })
    }
    if (isInitialSetup) options.push({ value: NEW_SERVER, label: 'Add new server…' })
    return options
  })

  function manageCredentials(): void {
    if (busy !== '') return
    credentialEditorOpen = true
  }

  function credentialUpdated(): void {
    if (existingConfig && selectedServer === existingConfig.baseUrl) {
      configuredCredentialOverride = { hasToken: true, authenticationState: 'valid' }
    }
    const remaining = servers.filter((server) => server.baseUrl !== effectiveUrl)
    servers = [...remaining, { baseUrl: effectiveUrl, authenticationState: 'valid' }]
    typesError = ''
  }

  async function closeCredentialEditor(): Promise<void> {
    credentialEditorOpen = false
    await tick()
    credentialButtonEl?.focus()
  }

  onMount(async () => {
    containerEl?.focus()
    existingConfig = initialConfig
    if (existingConfig) {
      selectedServer = existingConfig.baseUrl
      for (const buildType of existingConfig.buildTypes) {
        selected.set(buildType.id, buildType.label)
      }
    }
    try {
      const all = await window.api.keychainListCredentials()
      servers = all
        .filter((credential) => credential.provider === 'teamcity')
        .map((credential) => ({
          baseUrl: credential.baseUrl,
          authenticationState:
            credential.authenticationState === 'valid' ||
            credential.authenticationState === 'invalid'
              ? credential.authenticationState
              : 'unknown',
        }))
    } catch {
      servers = []
    }
    if (initialInvalid) {
      configLoadError = initialInvalid.message
      configLoadScope = initialInvalid.scope
    }
    if (existingConfig) {
      // Editing with a stored token: show the picker right away.
      if (canLoadTypes) void loadBuildTypes()
    } else if (servers.length > 0) {
      selectedServer = servers[0].baseUrl
    }
  })

  function handleKeydown(e: KeyboardEvent): void {
    if (e.key === 'Escape') {
      e.preventDefault()
      requestClose()
      return
    }
    if (e.key === 'Tab' && containerEl) cycleFocus(containerEl, e)
  }

  /** Every dismissal route funnels here: closing during an in-flight write would
      destroy the footer region its failure is routed to — the error would have NO
      surface (the success toast fires only after the await). Gated on `busy`, not
      `saving`, so Escape still works while a removal confirm is open. */
  function requestClose(): void {
    if (busy === '') closeDialog()
  }

  function selectServer(value: string): void {
    selectedServer = value
    testResult = ''
    typesLoaded = false
    serverTypes = []
    typesError = ''
    // Belongs to the save that produced it, and Save is about to target a
    // different server — same reason testResult and typesError reset here.
    saveError = ''
    // The selection belongs to the server it was loaded from — carrying it across
    // would let Save write these ids under a different baseUrl into the git-shared
    // config, where nothing cross-checks that the jobs exist on that server.
    selected.clear()
    // Restoring the repo's own saved selection is not "carrying a selection across
    // servers" — these ids belong to this baseUrl. A selection the user emptied by
    // unticking is never restored, because nothing re-seeds on reload.
    if (existingConfig && value === existingConfig.baseUrl) {
      for (const bt of existingConfig.buildTypes) selected.set(bt.id, bt.label)
    }
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
        `The token will be sent only to this address and, when saved, stored ${storage} for this server-scoped TeamCity integration. Only continue if you recognize it as your TeamCity server.` +
        (insecure
          ? ' Warning: this is a plain http:// address - the token would travel unencrypted.'
          : ''),
      confirmLabel: 'Continue',
    })
    if (ok) acknowledgedUrl = effectiveUrl
    return ok
  }

  async function testConnection(): Promise<void> {
    // Mirrors the button's aria-disabled — which does not stop clicks.
    if (!urlValid || !trimmedFormToken || testing) return
    if (!(await confirmDestination())) return
    testing = true
    testResult = ''
    try {
      await window.api.ciTestNewConnection(effectiveUrl, trimmedFormToken)
      testResult = 'success'
    } catch {
      testResult = 'fail'
    } finally {
      testing = false
    }
  }

  /** Stores a typed token (behind the destination gate) before first use. */
  async function ensureToken(): Promise<boolean> {
    if (serverHasToken && !trimmedFormToken) return true
    if (!trimmedFormToken) return false
    if (!(await confirmDestination())) return false
    try {
      await window.api.keychainSetCredentials('teamcity', effectiveUrl, trimmedFormToken)
      bumpCiCredentialTick()
    } catch (e) {
      // In-modal per the scrim rule — a toast would paint under this dialog. The
      // caller (Load available jobs) surfaces typesError right next to its button.
      typesError = ipcErrorMessage(e, 'Failed to save credentials')
      return false
    }
    servers = [
      ...servers.filter((server) => server.baseUrl !== effectiveUrl),
      { baseUrl: effectiveUrl, authenticationState: 'valid' },
    ]
    if (selectedServer === NEW_SERVER) selectedServer = effectiveUrl
    formToken = ''
    return true
  }

  async function loadBuildTypes(): Promise<void> {
    // Mirrors the button's aria-disabled — which does not stop clicks.
    if (!canLoadTypes || typesLoading) return
    if (!(await ensureToken())) return
    typesLoading = true
    typesError = ''
    // A stale WRITE failure must not sit through a reload it does not describe.
    saveError = ''
    try {
      serverTypes = await window.api.ciListBuildTypes(effectiveUrl)
      typesLoaded = true
    } catch (e) {
      const message = ipcErrorMessage(e, 'Could not load TeamCity jobs')
      let rejected = credentialRejected
      if (repoRoot && existingConfig && selectedServer === existingConfig.baseUrl) {
        try {
          const refreshed = await window.api.ciConfig(repoRoot)
          if (refreshed.credential) configuredCredentialOverride = refreshed.credential
          rejected = refreshed.credential?.authenticationState === 'invalid'
        } catch {
          // Keep the sanitized request error when the credential verdict cannot be refreshed.
        }
      }
      typesError = rejected ? '' : message
      serverTypes = []
      // A failed request says nothing about what exists on the server — keep the
      // picker, the stale-job warning and Save out of the "we know" state.
      typesLoaded = false
    } finally {
      typesLoading = false
    }
  }

  // What Save will actually write — only ids the server just confirmed, ordered by
  // the existing config (new entries appended) so an unrelated Save doesn't reshuffle
  // the team's sidebar rows. Gating the button on THIS instead of `selected.size`
  // keeps the enabled state and the request in agreement when every seeded id has
  // been deleted or re-ided on TeamCity.
  let effectiveBuildTypes = $derived.by(() => {
    const confirmed = serverTypes
      .filter((bt) => selected.has(bt.id))
      .map((bt) => ({ id: bt.id, label: selected.get(bt.id) || bt.name }))
    const savedOrder = new Map((existingConfig?.buildTypes ?? []).map((bt, i) => [bt.id, i]))
    return confirmed.sort(
      (a, b) =>
        (savedOrder.get(a.id) ?? Number.MAX_SAFE_INTEGER) -
        (savedOrder.get(b.id) ?? Number.MAX_SAFE_INTEGER),
    )
  })

  // Configured ids the server no longer returns — invisible in the picker (no
  // checkbox to untick), so they must be called out before Save silently drops them.
  // Only ever computed against a list the server actually returned: `loadBuildTypes`
  // resets `typesLoaded` on failure, so a dropped request can't read as "deleted".
  let missingBuildTypes = $derived(
    typesLoaded ? [...selected.keys()].filter((id) => !serverTypes.some((bt) => bt.id === id)) : [],
  )

  // True only when the SERVER is the reason nothing can be saved — none of the ids
  // in the committed config came back. Unticking every live job also empties
  // `effectiveBuildTypes`, but that is the user's own edit and must not be reported
  // as "your jobs are gone" (nor push them at Remove CI configuration).
  let allConfiguredStale = $derived(
    typesLoaded &&
      (existingConfig?.buildTypes.length ?? 0) > 0 &&
      (existingConfig?.buildTypes ?? []).every((bt) => !serverTypes.some((s) => s.id === bt.id)),
  )

  // ONE definition of "Save cannot run": aria-disabled does not stop clicks, so
  // saveConfiguration guards on exactly what the button renders as disabled.
  let saveBlocked = $derived(
    saving ||
      typesLoading ||
      !typesLoaded ||
      effectiveBuildTypes.length === 0 ||
      effectiveBuildTypes.length > CI_MAX_BUILD_TYPES ||
      !urlValid ||
      configLoadScope === 'file',
  )

  // The control row delegates this exact sentence to the footer, so every surface
  // uses one owner and wording changes cannot silently break that delegation.
  const URL_REQUIRED = 'Disabled: enter a valid TeamCity server URL first'

  // Per-control titles keep the full cascade so a blocked button never promises
  // an action it cannot perform. The shared rendered reason normally carries the
  // token precondition; invalid URL is delegated to the footer only when nothing
  // outranks it. A file-scope error does so durably, while the confirm and active
  // removal states do so transiently. Busy states live on the button itself
  // (label + aria-busy), as in CiServerForm's `formBlockedReason` split.
  let testBlockedTitle = $derived(
    testing ? 'Testing the connection…' : !urlValid ? URL_REQUIRED : '',
  )
  let loadBlockedTitle = $derived(
    typesLoading
      ? "Loading the server's jobs…"
      : !urlValid
        ? URL_REQUIRED
        : credentialRejected && trimmedFormToken.length === 0
          ? credentialGate.jobsReason
          : !canLoadTypes
            ? isInitialSetup
              ? 'Disabled: enter an access token first (or pick a server with one stored)'
              : credentialGate.jobsReason
            : '',
  )
  // One source for the Save tooltip and its assistive description keeps the two
  // explanations from drifting apart.
  let saveBlockedState = $derived.by((): { reason: string } => {
    if (configLoadScope === 'file') {
      return {
        reason:
          'Disabled: .canopy/config.json cannot be used, so the ci block cannot be written without overwriting the rest of the file',
      }
    }
    if (busy === 'remove') {
      return { reason: 'Disabled: the CI configuration is being removed' }
    }
    // `saving` without a `busy` action is the pre-confirm guard, and only
    // removeConfiguration awaits inside it (saveConfiguration sets both
    // synchronously) — so nothing has started yet. Ranked with `busy === 'remove'`
    // rather than last: both describe the removal the user just started, and a
    // standing precondition about Save must not out-rank the modal on screen.
    if (saving && busy === '') {
      return { reason: 'Disabled: confirm or dismiss the removal first' }
    }
    if (!urlValid) {
      return { reason: URL_REQUIRED }
    }
    if (credentialRejected || (!isInitialSetup && !credentialGate.canLoadJobs)) {
      return { reason: credentialGate.saveReason }
    }
    if (!canLoadTypes) {
      return {
        reason: isInitialSetup
          ? 'Disabled: enter an access token first (or pick a server with one stored)'
          : 'Disabled: add a token in Personal credentials first',
      }
    }
    if (typesLoading) {
      return { reason: "Disabled: loading the server's jobs…" }
    }
    if (!typesLoaded) {
      return {
        reason:
          'Disabled: click "Load available jobs" first - Canopy saves only jobs the server confirmed',
      }
    }
    if (effectiveBuildTypes.length > CI_MAX_BUILD_TYPES) {
      return {
        reason: `Disabled: at most ${CI_MAX_BUILD_TYPES} jobs can be configured - untick ${effectiveBuildTypes.length - CI_MAX_BUILD_TYPES}`,
      }
    }
    if (effectiveBuildTypes.length === 0) {
      return { reason: 'Disabled: tick at least one job below' }
    }
    if (busy === 'save') {
      return { reason: 'Disabled: an update is already in progress' }
    }
    return { reason: '' }
  })

  let serverBlockedReason = $derived(
    // Projection of the title, not a second copy. The URL term is delegated exactly
    // when the footer is stating it, so adding a higher-ranked footer term cannot
    // silently leave the row without an explanation.
    !typesLoading && (urlValid ? !canLoadTypes : saveBlockedState.reason !== URL_REQUIRED)
      ? loadBlockedTitle
      : '',
  )

  function toggleType(bt: ServerBuildType): void {
    if (selected.has(bt.id)) selected.delete(bt.id)
    else selected.set(bt.id, selected.get(bt.id) ?? bt.name)
    // A stale save failure must not sit next to a Save that now does something else.
    saveError = ''
  }

  function setLabel(id: string, label: string): void {
    selected.set(id, label)
    saveError = ''
  }

  async function saveConfiguration(): Promise<void> {
    // Mirrors the button's aria-disabled: the cap and the file-scope block are
    // real preconditions, and aria-disabled does not stop a click.
    if (!repoRoot || saveBlocked) return
    // No confirm on this path — the guard and the visible state start together.
    saving = true
    busy = 'save'
    saveError = ''
    try {
      await window.api.ciSaveConfig(repoRoot, {
        baseUrl: effectiveUrl,
        // Labels stay from `selected`: they are the user's own editable sidebar
        // labels, not mirrors of the server name.
        buildTypes: effectiveBuildTypes,
      })
      await loadCiRepoConfig(repoRoot)
      addToast('CI configuration saved - commit .canopy/config.json to share it')
      closeDialog()
    } catch (e) {
      saveError = ipcErrorMessage(e, 'Failed to save CI configuration')
    } finally {
      saving = false
      busy = ''
    }
  }

  async function removeConfiguration(): Promise<void> {
    if (!repoRoot || !existingConfig || saving) return
    // The SHARED `saving` guard, set before the confirm: it blocks a second
    // confirm from a double-click AND takes Save out of the enabled set, so two
    // read-modify-write passes over the git-shared .canopy/config.json cannot
    // overlap from this dialog (the main process serializes them per repo too).
    // The VISIBLE busy state waits for the answer — a "Saving…" label while the
    // user is still deciding describes work that has not started, and disabling
    // the activated button would blur it, dropping Cancel's focus restore.
    saving = true
    // Clear a previous SAVE failure before the confirm — declining it must not
    // leave that stale message sitting under the Remove button.
    saveError = ''
    try {
      const ok = await confirm({
        title: 'Remove CI configuration',
        message: `Remove the TeamCity configuration (${existingConfig.baseUrl}) from this repository?`,
        details:
          'Removes the ci block from the git-tracked .canopy/config.json - after committing, the whole team loses the CI rows. Your stored token stays (Settings > CI connections).',
        confirmLabel: 'Remove configuration',
        destructive: true,
      })
      if (!ok) return
      busy = 'remove'
      await window.api.ciSaveConfig(repoRoot, null)
      await loadCiRepoConfig(repoRoot)
      addToast('CI configuration removed')
      closeDialog()
    } catch (e) {
      saveError = ipcErrorMessage(e, 'Failed to remove CI configuration')
    } finally {
      saving = false
      busy = ''
    }
  }
</script>

<!-- svelte-ignore a11y_no_static_element_interactions -->
<div
  class="fixed inset-0 z-overlay flex justify-center items-center bg-scrim"
  onkeydown={handleKeydown}
  onmousedown={requestClose}
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
        <h2 class="text-lg font-semibold text-text m-0 leading-tight">CI/CD - TeamCity</h2>
        <p class="text-xs text-text-muted m-0 leading-snug">
          The server and the available build configurations are shared with your team via
          <code class="font-mono">.canopy/config.json</code> in this repository. Tokens stay on this machine.
        </p>
      </div>
      <button
        type="button"
        class="flex items-center justify-center size-7 rounded-md bg-transparent border-0 text-text-muted cursor-pointer hover:bg-hover hover:text-text shrink-0 aria-disabled:opacity-50 aria-disabled:cursor-default aria-disabled:hover:bg-transparent aria-disabled:hover:text-text-muted"
        onclick={requestClose}
        aria-disabled={busy !== ''}
        aria-label="Close"
        title={busy !== '' ? 'Disabled while an update is writing .canopy/config.json' : 'Close'}
      >
        <X size={16} />
      </button>
    </header>

    <div class="flex-1 overflow-y-auto px-6 py-5 flex flex-col gap-4">
      {#if !repoRoot}
        <p class="text-sm text-text-faint m-0">Open a repository first.</p>
      {:else}
        <!-- Persistent region: onMount resolves ciConfig before this text lands, so
             the wrapper must outlive the content or the mutation is never announced. -->
        <div role="status" class:sr-only={!configLoadError}>
          {#if configLoadError}
            <p class="m-0 text-xs text-warning-text leading-snug" title={configLoadError}>
              <!-- The separator lives INSIDE each branch: the unknown-scope catch
                   path renders just the message, not a dangling em dash. -->
              {configLoadError}
              {#if configLoadScope === 'file'}
                - fix <code class="font-mono">.canopy/config.json</code> by hand; Save is disabled here
                because writing would require reading the file first (nothing is ever re-initialized over
                it).
              {:else if configLoadScope === 'block'}
                - pick the server and jobs below and Save to replace the invalid
                <code class="font-mono">ci</code> block - the rest of the file is untouched.
              {/if}
            </p>
          {/if}
        </div>
        <section class="rounded-lg border border-border-subtle p-4 flex flex-col gap-3">
          <div>
            <h3 class="m-0 text-sm font-semibold text-text">Shared TeamCity server</h3>
            <p class="m-0 mt-0.5 text-xs text-text-muted leading-snug">
              The selected server is stored for everyone in this repository's
              <code class="font-mono">.canopy/config.json</code>.
            </p>
          </div>
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
        </section>

        <section class="rounded-lg border border-border-subtle p-4 flex flex-col gap-3">
          <div>
            <h3 class="m-0 text-sm font-semibold text-text">Personal credentials</h3>
            <p class="m-0 mt-0.5 text-xs text-text-muted leading-snug">
              Stored only on this machine and never written to
              <code class="font-mono">.canopy/config.json</code>.
            </p>
          </div>

          {#if !isInitialSetup}
            <div
              class="flex items-center justify-between gap-3 rounded-md border border-border bg-bg-input px-2.5 py-2"
            >
              <div class="min-w-0">
                <div
                  class="text-xs font-medium"
                  class:text-danger-text={!serverHasToken || credentialRejected}
                  class:text-text={serverHasToken && !credentialRejected}
                >
                  {credentialGate.credentialLabel}
                </div>
                <div class="truncate text-xs text-text-muted" title={effectiveUrl}>
                  {effectiveUrl}
                </div>
              </div>
              <button
                bind:this={credentialButtonEl}
                type="button"
                class="shrink-0 px-2 py-1 rounded-md border border-border bg-transparent text-xs text-text-secondary cursor-pointer hover:bg-hover"
                onclick={manageCredentials}
                aria-disabled={busy !== ''}
              >
                {selectedServer === existingConfig?.baseUrl
                  ? serverHasToken
                    ? 'Update token'
                    : 'Add credentials'
                  : 'Manage credentials'}
              </button>
            </div>
          {:else if !serverHasToken || credentialRejected || selectedServer === NEW_SERVER}
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
                title="Stored for this server-scoped CI integration on your machine - never written to your repository"
              />
              <div class="mt-1">
                <CredentialStorageNote
                  provider="teamcity"
                  baseUrl={urlValid ? effectiveUrl : undefined}
                  sharingNote={false}
                />
              </div>
            </div>
          {:else}
            <div
              class="flex items-center gap-3 rounded-md border border-border bg-bg-input px-2.5 py-2"
            >
              <div class="min-w-0">
                <div
                  class="text-xs font-medium"
                  class:text-danger-text={credentialRejected}
                  class:text-text={!credentialRejected}
                >
                  {credentialGate.credentialLabel}
                </div>
                <div class="truncate text-xs text-text-muted" title={effectiveUrl}>
                  {effectiveUrl}
                </div>
              </div>
            </div>
          {/if}
        </section>

        <section class="rounded-lg border border-border-subtle p-4 flex flex-col gap-3">
          <div>
            <h3 class="m-0 text-sm font-semibold text-text">Shared jobs</h3>
            <p class="m-0 mt-0.5 text-xs text-text-muted leading-snug">
              Select the jobs shown to everyone through this repository's
              <code class="font-mono">.canopy/config.json</code>.
            </p>
          </div>

          <div class="flex items-center gap-1.5">
            {#if trimmedFormToken}
              <button
                type="button"
                class="px-3 py-1 rounded-md text-sm font-inherit cursor-pointer border border-border bg-bg-input text-text-secondary hover:bg-hover-strong hover:text-text aria-disabled:opacity-50 aria-disabled:cursor-default aria-disabled:hover:bg-bg-input aria-disabled:hover:text-text-secondary"
                onclick={testConnection}
                aria-disabled={testing || !urlValid}
                aria-busy={testing}
                aria-describedby={serverBlockedReason ? 'ci-server-blocked' : undefined}
                title={testBlockedTitle ||
                  'Check the connection against the server - nothing is saved'}
              >
                {testing ? 'Testing…' : 'Test'}
              </button>
            {/if}
            <button
              type="button"
              class="px-3 py-1 rounded-md text-sm font-inherit cursor-pointer border border-border bg-bg-input text-text-secondary hover:bg-hover-strong hover:text-text aria-disabled:opacity-50 aria-disabled:cursor-default aria-disabled:hover:bg-bg-input aria-disabled:hover:text-text-secondary"
              onclick={loadBuildTypes}
              aria-disabled={typesLoading || !canLoadTypes}
              aria-busy={typesLoading}
              aria-describedby={serverBlockedReason ? 'ci-server-blocked' : undefined}
              title={loadBlockedTitle ||
                'Saves the token (when entered) and fetches the list of jobs (build configurations) from the TeamCity server'}
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

          <!-- Keep the tooltip text available to assistive technology without
             duplicating it as a visible label below the action. -->
          <div class="sr-only">
            {#if serverBlockedReason}
              <span id="ci-server-blocked" class="text-xs text-text-secondary break-words"
                >{serverBlockedReason}</span
              >
            {/if}
          </div>

          <!-- Persistent region: a load or keychain failure lands as a mutation —
             a span mounted together with its content is never announced. -->
          <div class:sr-only={!typesError} role="status">
            {#if typesError}
              <span class="text-xs text-danger-text">{typesError}</span>
            {/if}
          </div>

          <!-- Persistent live region for the modal's LIFETIME: a wrapper mounted in the
             same render pass as its content is skipped by screen readers, and every
             path that changes the message resets typesLoaded first — so the region
             must outlive the conditional chain below. -->
          <div
            role="status"
            id="ci-save-warnings"
            class:sr-only={!existingConfig?.droppedInvalid &&
              !existingConfig?.droppedOverCap &&
              !(typesLoaded && missingBuildTypes.length > 0)}
          >
            {#if existingConfig?.droppedInvalid}
              <!-- Recovery is correcting the FILE: these are not TeamCity ids, so
                 the picker below can never show them — "tick to keep" would be
                 impossible advice here. -->
              {@const inv = existingConfig.droppedInvalid}
              <p class="m-0 text-xs text-warning-text leading-snug break-words">
                {inv.count} hand-edited
                {inv.count === 1 ? 'entry has an invalid id' : 'entries have invalid ids'}
                ({inv.ids.join(', ')}{inv.count > inv.ids.length
                  ? ` and ${inv.count - inv.ids.length} more`
                  : ''}) - not TeamCity ids, so they cannot appear below. Correct them in
                <code class="font-mono">.canopy/config.json</code> to keep them; saving without doing
                so drops them.
              </p>
            {/if}
            {#if existingConfig?.droppedOverCap}
              <!-- Recovery is re-ticking — but only for ids the SERVER still has:
                 parseCiConfig verified the charset, not existence, and an id
                 deleted on TeamCity is absent from the picker AND from the
                 stale-jobs warning (which only sees `selected`). Until the list
                 is loaded, existence is genuinely unknown. -->
              {@const cap = existingConfig.droppedOverCap}
              {@const capPresent = cap.ids.filter((id) => serverTypes.some((bt) => bt.id === id))}
              {@const capGone = cap.ids.filter((id) => !serverTypes.some((bt) => bt.id === id))}
              {@const shorten = (id: string): string =>
                id.length > 80 ? `${id.slice(0, 80)}…` : id}
              <p class="m-0 text-xs text-warning-text leading-snug break-words">
                {cap.count} hand-edited
                {cap.count === 1 ? 'entry is' : 'entries are'} past the
                {CI_MAX_BUILD_TYPES}-job cap and not selected{cap.count > cap.ids.length
                  ? ` (showing ${cap.ids.length} of ${cap.count})`
                  : ''}.
                {#if !typesLoaded}
                  <!-- Existence is genuinely unknown until the server list loads —
                     promising a re-tick here would be an optimistic default the
                     ids cannot back up yet. -->
                  Load the available jobs to see which of these can still be ticked:
                  {cap.ids.map(shorten).join(', ')}.
                {:else}
                  {#if capPresent.length > 0}
                    Untick another job below first, then tick these to keep them: {capPresent
                      .map(shorten)
                      .join(', ')}.
                  {/if}
                  {#if capGone.length > 0}
                    No longer on this server - saving drops them and there is nothing to re-tick:
                    {capGone.map(shorten).join(', ')}.
                  {/if}
                {/if}
                Or trim the hand-edited list; saving writes only the selection below.
              </p>
            {/if}
            {#if typesLoaded && missingBuildTypes.length > 0}
              {@const missingNames = missingBuildTypes
                .map((id) => {
                  const label = selected.get(id)
                  return label && label !== id ? `${label} (${id})` : id
                })
                .join(', ')}
              <p class="m-0 text-xs text-warning-text leading-snug break-words">
                {#if allConfiguredStale && effectiveBuildTypes.length === 0}
                  None of this repository's configured jobs exist on this server any more ({missingNames}).
                  Save is disabled until you tick at least one job below - or use
                  <strong>Remove CI configuration</strong> to drop the
                  <code class="font-mono">ci</code> block entirely.
                {:else if effectiveBuildTypes.length > 0}
                  {missingBuildTypes.length} configured
                  {missingBuildTypes.length === 1 ? 'job is' : 'jobs are'} no longer on this server ({missingNames}).
                  Saving drops
                  {missingBuildTypes.length === 1 ? 'it' : 'them'} from
                  <code class="font-mono">.canopy/config.json</code>.
                {:else}
                  {missingBuildTypes.length} configured
                  {missingBuildTypes.length === 1 ? 'job is' : 'jobs are'} no longer on this server ({missingNames}).
                  Tick at least one job below to save - the missing
                  {missingBuildTypes.length === 1 ? 'entry is' : 'entries are'} dropped from
                  <code class="font-mono">.canopy/config.json</code> when you do.
                {/if}
              </p>
            {/if}
          </div>

          {#if typesLoading && !typesLoaded}
            <div class="flex items-center gap-2 text-sm text-text-faint">
              <LoaderCircle size={13} class="animate-spin-slow motion-reduce:animate-none" />
              Loading available jobs…
            </div>
          {:else if typesLoaded}
            <!-- During a RELOAD the previous list stays on screen but visibly inert —
               Save is disabled above and the picker dims until the fresh list lands,
               so the user can't edit rows that are about to be replaced. -->
            <div class={typesLoading ? 'opacity-50 pointer-events-none' : ''}>
              <CiJobPicker
                {serverTypes}
                {selected}
                onToggle={toggleType}
                onLabelChange={setLabel}
              />
            </div>
          {/if}
        </section>
      {/if}
    </div>

    <footer
      class="px-6 py-3 border-t border-border-subtle shrink-0 flex items-center justify-between gap-2"
    >
      <div>
        {#if existingConfig}
          <!-- aria-disabled (not disabled): a real disabled would blur the button
               mid-write and strand ConfirmDialog's focus restore on <body>. The
               busy feedback is the spinner + dimming via aria-disabled: variants —
               enabled:/disabled: variants key off the real attribute and would be
               dead here. Clicks during the write no-op via the `saving` guard. -->
          <button
            type="button"
            class="flex items-center gap-1 px-2 py-1 rounded-md border-0 bg-transparent text-text-faint text-xs font-inherit cursor-pointer hover:text-danger-text aria-disabled:opacity-50 aria-disabled:cursor-default aria-disabled:hover:text-text-faint"
            onclick={removeConfiguration}
            aria-disabled={busy !== ''}
            aria-busy={busy === 'remove'}
            title={busy !== ''
              ? 'Disabled while an update is writing .canopy/config.json'
              : 'Removes the ci block from the git-tracked .canopy/config.json'}
          >
            {#if busy === 'remove'}
              <LoaderCircle size={12} class="animate-spin-slow motion-reduce:animate-none" />
            {:else}
              <Trash2 size={12} />
            {/if}
            Remove CI configuration
          </button>
        {/if}
      </div>
      <!-- Stacks the save failure and the blocked-reason line — both wrap rather
           than truncate: CiApiError can carry TeamCity's response body, and the
           one message explaining why a git-shared file was not written must be
           fully readable. -->
      <div class="flex-1 min-w-0 flex flex-col gap-0.5">
        <!-- Persistent region: a failed save/remove lands here as a mutation — the
             toast layer (z-banner) paints UNDER this modal's scrim (z-overlay). -->
        <div class="text-xs text-danger-text break-words" aria-live="polite">
          {saveError}
        </div>
        <!-- Keep the tooltip text available to assistive technology without
             duplicating it as a visible footer label. -->
        {#if saveBlockedState.reason}
          <span id="ci-save-blocked" class="sr-only">{saveBlockedState.reason}</span>
        {/if}
      </div>
      <div class="flex items-center gap-1.5">
        <button
          type="button"
          class="px-3 py-1 rounded-md text-sm font-inherit cursor-pointer border border-border bg-transparent text-text-secondary hover:bg-hover hover:text-text aria-disabled:opacity-50 aria-disabled:cursor-default aria-disabled:hover:bg-transparent aria-disabled:hover:text-text-secondary"
          onclick={requestClose}
          aria-disabled={busy !== ''}
          title={busy !== ''
            ? 'Disabled while an update is writing .canopy/config.json'
            : 'Close without saving'}>Cancel</button
        >
        <button
          type="button"
          class="px-3 py-1 rounded-md text-sm font-inherit cursor-pointer border-0 bg-accent-bg text-accent-text hover:bg-accent-bg-hover aria-disabled:opacity-50 aria-disabled:cursor-default aria-disabled:hover:bg-accent-bg"
          onclick={saveConfiguration}
          aria-disabled={saveBlocked}
          aria-describedby={saveBlockedState.reason
            ? 'ci-save-blocked'
            : missingBuildTypes.length > 0 ||
                existingConfig?.droppedInvalid ||
                existingConfig?.droppedOverCap
              ? 'ci-save-warnings'
              : undefined}
          title={saveBlockedState.reason ||
            'Writes the ci block to .canopy/config.json - commit it to share with the team'}
          >{busy === 'save' ? 'Saving…' : 'Save configuration'}</button
        >
      </div>
    </footer>
  </div>
</div>

{#if credentialEditorOpen && existingConfig && repoRoot && urlValid}
  <CiCredentialModal
    {repoRoot}
    config={{ ...existingConfig, baseUrl: effectiveUrl }}
    onClose={closeCredentialEditor}
    onUpdated={credentialUpdated}
  />
{/if}
