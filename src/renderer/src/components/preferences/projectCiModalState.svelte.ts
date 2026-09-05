import { onMount, tick } from 'svelte'
import { SvelteMap } from 'svelte/reactivity'
import { closeDialog, confirm } from '../../lib/stores/dialogs.svelte'
import { addToast } from '../../lib/stores/toast.svelte'
import { bumpCiCredentialTick, loadCiRepoConfig } from '../../lib/stores/ci.svelte'
import { cycleFocus } from '../../lib/a11y/focusTrap'
import { canUseTeamCityCredential, teamCityCredentialGate } from '../../lib/ci/credentialGate'
import { ipcErrorMessage } from '../../lib/ci/errors'
import { CI_MAX_BUILD_TYPES } from '../../lib/ci/limits'
import type { CiCredentialStatus, TeamCityCiRepoConfigInfo } from '../../lib/ci/types'
import { credentialStorageClause } from './_partials/credentialStorage'
import { createLatestRequestGuard } from '../../lib/async/latestRequest'

export interface InvalidCiConfig {
  scope: 'file' | 'block'
  message: string
  provider?: 'teamcity' | 'github-actions'
}

export interface ServerBuildType {
  id: string
  name: string
  projectName: string
}

// Keep this factory inferred so the exported ReturnType stays aligned with its reactive getters.
// eslint-disable-next-line @typescript-eslint/explicit-function-return-type
export function createProjectCiModalState({
  repoRoot,
  initialConfig,
  initialCredential,
  initialInvalid,
}: {
  repoRoot: string
  initialConfig: TeamCityCiRepoConfigInfo | null
  initialCredential?: CiCredentialStatus
  initialInvalid?: InvalidCiConfig
}) {
  // Per-repo CI/CD configuration (TeamCity) for the ACTIVE worktree — the analogue of
  // the Project tracker modal. The server + selected build configurations are written
  // to the git-tracked .canopy/config.json (team-shared); tokens stay personal and
  // are managed in Settings → CI connections.

  const NEW_SERVER = '__new__'

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
  const trimmedFormToken = $derived(formToken.trim())
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
  const typesRequestGuard = createLatestRequestGuard()
  const selected = new SvelteMap<string, string>()

  const effectiveUrl = $derived(
    selectedServer === NEW_SERVER ? newUrl.trim().replace(/\/$/, '') : selectedServer,
  )
  const urlValid = $derived(/^https?:\/\/\S+$/i.test(effectiveUrl))
  const selectedCredential = $derived.by((): CiCredentialStatus => {
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
  const credentialGate = $derived(teamCityCredentialGate(selectedCredential))
  const serverHasToken = $derived(selectedCredential.hasToken)
  const credentialRejected = $derived(
    selectedCredential.hasToken && selectedCredential.authenticationState === 'invalid',
  )
  const isInitialSetup = $derived(initialConfig === null)
  const canLoadTypes = $derived(
    urlValid && canUseTeamCityCredential(selectedCredential, trimmedFormToken.length > 0),
  )

  const serverOptions = $derived.by(() => {
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
      configuredCredentialOverride = {
        hasToken: true,
        authenticationState: 'valid',
        approvalRequired: true,
      }
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
    typesRequestGuard.invalidate()
    typesLoading = false
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
  async function confirmDestination(destinationUrl = effectiveUrl): Promise<boolean> {
    if (destinationUrl === acknowledgedUrl) return true
    const encryptionAvailable = await window.api
      .isCredentialEncryptionAvailable()
      .catch(() => false)
    const storage = credentialStorageClause(window.api.platform, encryptionAvailable)
    const insecure = destinationUrl.startsWith('http://')
    const ok = await confirm({
      title: 'Confirm CI server address',
      message: `Send your TeamCity token to ${destinationUrl}?`,
      details:
        `The token will be sent only to this address and, when saved, stored ${storage} for this server-scoped TeamCity integration. Only continue if you recognize it as your TeamCity server.` +
        (insecure
          ? ' Warning: this is a plain http:// address - the token would travel unencrypted.'
          : ''),
      confirmLabel: 'Continue',
    })
    if (ok) acknowledgedUrl = destinationUrl
    return ok
  }

  async function testConnection(): Promise<void> {
    // Mirrors the button's aria-disabled — which does not stop clicks.
    if (!urlValid || !trimmedFormToken || testing || typesLoading) return
    const destinationUrl = effectiveUrl
    const token = trimmedFormToken
    testing = true
    testResult = ''
    try {
      if (!(await confirmDestination(destinationUrl))) return
      await window.api.ciTestNewConnection(destinationUrl, token)
      if (effectiveUrl === destinationUrl && trimmedFormToken === token) testResult = 'success'
    } catch {
      if (effectiveUrl === destinationUrl && trimmedFormToken === token) testResult = 'fail'
    } finally {
      testing = false
    }
  }

  /** Stores a typed token (behind the destination gate) before first use. */
  async function ensureToken(destinationUrl: string, token: string): Promise<boolean> {
    if (serverHasToken && !token) return true
    if (!token) return false
    if (!(await confirmDestination(destinationUrl))) return false
    try {
      await window.api.keychainSetCredentials('teamcity', destinationUrl, token)
      bumpCiCredentialTick()
    } catch (e) {
      // In-modal per the scrim rule — a toast would paint under this dialog. The
      // caller (Load available jobs) surfaces typesError right next to its button.
      if (effectiveUrl === destinationUrl && trimmedFormToken === token) {
        typesError = ipcErrorMessage(e, 'Failed to save credentials')
      }
      return false
    }
    servers = [
      ...servers.filter((server) => server.baseUrl !== destinationUrl),
      { baseUrl: destinationUrl, authenticationState: 'valid' },
    ]
    const formStillCurrent = effectiveUrl === destinationUrl && trimmedFormToken === token
    if (selectedServer === NEW_SERVER && formStillCurrent) {
      selectedServer = destinationUrl
    }
    if (formStillCurrent) formToken = ''
    return formStillCurrent
  }

  async function loadBuildTypes(): Promise<void> {
    // Mirrors the button's aria-disabled — which does not stop clicks.
    if (!canLoadTypes || typesLoading || testing) return
    const requestedUrl = effectiveUrl
    const requestedToken = trimmedFormToken
    const request = typesRequestGuard.begin(requestedUrl)
    // Acquire the re-entrancy guard before confirmation or keychain I/O. Two rapid activations
    // must not replace the singleton confirmation and strand its first Promise.
    typesLoading = true
    typesError = ''
    // A stale WRITE failure must not sit through a reload it does not describe.
    saveError = ''
    try {
      if (!(await ensureToken(requestedUrl, requestedToken))) return
      if (!typesRequestGuard.isCurrent(request, effectiveUrl)) return
      const response = await window.api.ciListBuildTypes(repoRoot, requestedUrl)
      if (!typesRequestGuard.isCurrent(request, effectiveUrl)) return
      serverTypes = response
      typesLoaded = true
    } catch (e) {
      if (!typesRequestGuard.isCurrent(request, effectiveUrl)) return
      const message = ipcErrorMessage(e, 'Could not load TeamCity jobs')
      let rejected = credentialRejected
      if (repoRoot && existingConfig && selectedServer === existingConfig.baseUrl) {
        try {
          const refreshed = await window.api.ciConfig(repoRoot)
          if (!typesRequestGuard.isCurrent(request, effectiveUrl)) return
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
      if (typesRequestGuard.isLatest(request)) typesLoading = false
    }
  }

  // What Save will actually write — only ids the server just confirmed, ordered by
  // the existing config (new entries appended) so an unrelated Save doesn't reshuffle
  // the team's sidebar rows. Gating the button on THIS instead of `selected.size`
  // keeps the enabled state and the request in agreement when every seeded id has
  // been deleted or re-ided on TeamCity.
  const effectiveBuildTypes = $derived.by(() => {
    const confirmed = serverTypes
      .filter((bt) => selected.has(bt.id))
      .map((bt) => ({ id: bt.id, label: selected.get(bt.id) || bt.name }))
    const savedOrder = new SvelteMap((existingConfig?.buildTypes ?? []).map((bt, i) => [bt.id, i]))
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
  const missingBuildTypes = $derived(
    typesLoaded ? [...selected.keys()].filter((id) => !serverTypes.some((bt) => bt.id === id)) : [],
  )

  // True only when the SERVER is the reason nothing can be saved — none of the ids
  // in the committed config came back. Unticking every live job also empties
  // `effectiveBuildTypes`, but that is the user's own edit and must not be reported
  // as "your jobs are gone" (nor push them at Remove CI configuration).
  const allConfiguredStale = $derived(
    typesLoaded &&
      (existingConfig?.buildTypes.length ?? 0) > 0 &&
      (existingConfig?.buildTypes ?? []).every((bt) => !serverTypes.some((s) => s.id === bt.id)),
  )

  // ONE definition of "Save cannot run": aria-disabled does not stop clicks, so
  // saveConfiguration guards on exactly what the button renders as disabled.
  const saveBlocked = $derived(
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
  const testBlockedTitle = $derived(
    testing
      ? 'Testing the connection…'
      : typesLoading
        ? "Loading the server's jobs…"
        : !urlValid
          ? URL_REQUIRED
          : '',
  )
  const loadBlockedTitle = $derived(
    typesLoading
      ? "Loading the server's jobs…"
      : testing
        ? 'Testing the connection…'
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
  const saveBlockedState = $derived.by((): { reason: string; severity?: 'warn' } => {
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

  const serverBlockedReason = $derived(
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

  return {
    repoRoot,
    get containerEl() {
      return containerEl
    },
    set containerEl(value: HTMLElement | undefined) {
      containerEl = value
    },
    get existingConfig() {
      return existingConfig
    },
    get configLoadError() {
      return configLoadError
    },
    get configLoadScope() {
      return configLoadScope
    },
    get saveError() {
      return saveError
    },
    get selectedServer() {
      return selectedServer
    },
    get newServerValue() {
      return NEW_SERVER
    },
    get newUrl() {
      return newUrl
    },
    set newUrl(value: string) {
      newUrl = value
    },
    get formToken() {
      return formToken
    },
    set formToken(value: string) {
      formToken = value
    },
    get trimmedFormToken() {
      return trimmedFormToken
    },
    get testing() {
      return testing
    },
    get testResult() {
      return testResult
    },
    get saving() {
      return saving
    },
    get busy() {
      return busy
    },
    get credentialEditorOpen() {
      return credentialEditorOpen
    },
    get credentialButtonEl() {
      return credentialButtonEl
    },
    set credentialButtonEl(value: HTMLButtonElement | undefined) {
      credentialButtonEl = value
    },
    get serverTypes() {
      return serverTypes
    },
    get typesLoading() {
      return typesLoading
    },
    get typesError() {
      return typesError
    },
    get typesLoaded() {
      return typesLoaded
    },
    selected,
    get effectiveUrl() {
      return effectiveUrl
    },
    get urlValid() {
      return urlValid
    },
    get credentialGate() {
      return credentialGate
    },
    get serverHasToken() {
      return serverHasToken
    },
    get credentialRejected() {
      return credentialRejected
    },
    get isInitialSetup() {
      return isInitialSetup
    },
    get canLoadTypes() {
      return canLoadTypes
    },
    get serverOptions() {
      return serverOptions
    },
    get effectiveBuildTypes() {
      return effectiveBuildTypes
    },
    get missingBuildTypes() {
      return missingBuildTypes
    },
    get allConfiguredStale() {
      return allConfiguredStale
    },
    get saveBlocked() {
      return saveBlocked
    },
    get testBlockedTitle() {
      return testBlockedTitle
    },
    get loadBlockedTitle() {
      return loadBlockedTitle
    },
    get saveBlockedState() {
      return saveBlockedState
    },
    get serverBlockedReason() {
      return serverBlockedReason
    },
    manageCredentials,
    credentialUpdated,
    closeCredentialEditor,
    handleKeydown,
    requestClose,
    selectServer,
    testConnection,
    loadBuildTypes,
    toggleType,
    setLabel,
    saveConfiguration,
    removeConfiguration,
  }
}

export type ProjectCiModalState = ReturnType<typeof createProjectCiModalState>
