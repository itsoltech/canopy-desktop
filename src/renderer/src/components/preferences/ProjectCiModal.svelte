<script lang="ts">
  import { onMount } from 'svelte'
  import { Check, LoaderCircle, Trash2, X } from '@lucide/svelte'
  import { SvelteMap } from 'svelte/reactivity'
  import { closeDialog, confirm } from '../../lib/stores/dialogs.svelte'
  import { workspaceState } from '../../lib/stores/workspace.svelte'
  import { addToast } from '../../lib/stores/toast.svelte'
  import { loadCiRepoConfig } from '../../lib/stores/ci.svelte'
  import { cycleFocus } from '../../lib/a11y/focusTrap'
  import { CI_MAX_BUILD_TYPES } from '../../lib/ci/limits'
  import type { CiRepoConfigInfo } from '../../lib/ci/types'
  import CustomSelect from '../shared/CustomSelect.svelte'
  import CiJobPicker from '../ci/CiJobPicker.svelte'
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

  // The SHARED shape (renderer mirror of the preload CiConfigInfo) — an inline
  // copy already drifted once, reading a field the copy didn't declare.
  let existingConfig = $state<CiRepoConfigInfo | null>(null)
  /** Set when a ci block exists but could not be used — shown so the advertised
      fix-and-re-save path names what is wrong. The scope picks the ONE recovery
      route to offer (file → hand-edit, Save disabled; block → re-save replaces). */
  let configLoadError = $state('')
  let configLoadScope = $state<'file' | 'block' | ''>('')
  /** Save/remove failure — surfaced in the footer: a toast would render UNDER
      this modal's scrim (z-banner 9999 < z-overlay 10000) and be unclickable. */
  let saveError = $state('')
  let servers = $state<Array<{ baseUrl: string }>>([])
  let selectedServer = $state<string>(NEW_SERVER)
  let newUrl = $state('')
  let formToken = $state('')
  let testing = $state(false)
  let testResult = $state<'success' | 'fail' | ''>('')
  let saving = $state(false)
  // VISIBLE busy state, per action — set only after a confirm resolves, so
  // "Saving…" never shows (and Remove never dims/blurs itself) while the user is
  // deciding, and each button renders only ITS OWN spinner: one shared flag made
  // a Save animate Remove's icon, announcing a removal that wasn't happening.
  // `saving` stays the pre-confirm re-entrancy guard shared by both actions.
  let busy = $state<'' | 'save' | 'remove'>('')

  let serverTypes = $state<ServerBuildType[]>([])
  let typesLoading = $state(false)
  let typesError = $state('')
  let typesLoaded = $state(false)
  const selected = new SvelteMap<string, string>()

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
        const res = await window.api.ciConfig(repoRoot)
        existingConfig = res.config
        // A block that EXISTS but cannot be used — this modal is the advertised
        // fix path, so it must show what is wrong instead of opening as if the
        // repo had never been configured.
        if (res.invalid) {
          configLoadError = res.invalid.message
          configLoadScope = res.invalid.scope
        }
      } catch (e) {
        existingConfig = null
        configLoadError =
          e instanceof Error ? e.message : "Could not read this repository's CI configuration"
      }
    }
    if (existingConfig) {
      selectedServer = existingConfig.baseUrl
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
    // Mirrors the button's aria-disabled — which does not stop clicks.
    if (!urlValid || !formToken || testing) return
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
      // In-modal per the scrim rule — a toast would paint under this dialog. The
      // caller (Load available jobs) surfaces typesError right next to its button.
      typesError = e instanceof Error ? e.message : 'Failed to save credentials'
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
      typesError = e instanceof Error ? e.message : 'Failed to load build configurations'
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

  // Per-control titles keep the full cascade so a blocked button never promises
  // an action it cannot perform. The shared rendered reason normally carries the
  // token precondition; invalid URL is delegated to the footer only when nothing
  // outranks it. A file-scope error does so durably, while the confirm and active
  // removal states do so transiently. Busy states live on the button itself
  // (label + aria-busy), as in CiServerForm's `formBlockedReason` split.
  let testBlockedTitle = $derived(
    testing
      ? 'Testing the connection…'
      : !urlValid
        ? 'Disabled: enter a valid TeamCity server URL first'
        : '',
  )
  let loadBlockedTitle = $derived(
    typesLoading
      ? "Loading the server's jobs…"
      : !urlValid
        ? 'Disabled: enter a valid TeamCity server URL first'
        : !canLoadTypes
          ? 'Disabled: enter an access token first (or pick a server with one stored)'
          : '',
  )
  let serverBlockedReason = $derived(
    // Projection of the title, not a second copy. The file-scope fallback exposes
    // the URL term while the footer is occupied; transient removal states keep
    // their own confirm or busy surface.
    !typesLoading && (urlValid ? !canLoadTypes : configLoadScope === 'file')
      ? loadBlockedTitle
      : '',
  )

  // Why Save cannot run, and how loud to say it — ONE pass, so the sentence and
  // its colour cannot disagree (a flat severity disjunction paired a next-step
  // sentence with the warning colour whenever an earlier cascade term won while
  // over-cap was also true). Warning colour is only for the two terms that
  // describe something WRONG (a file that cannot be written, a selection over
  // the cap); the rest are next-step states, two of which render on mount, and
  // painting those like the dropped-entry warnings above would devalue those.
  let saveBlockedState = $derived.by((): { reason: string; severity: 'warn' | 'info' } => {
    if (configLoadScope === 'file') {
      return {
        reason:
          'Disabled: .canopy/config.json cannot be used, so the ci block cannot be written without overwriting the rest of the file',
        severity: 'warn',
      }
    }
    if (busy === 'remove') {
      return { reason: 'Disabled: the CI configuration is being removed', severity: 'info' }
    }
    // `saving` without a `busy` action is the pre-confirm guard, and only
    // removeConfiguration awaits inside it (saveConfiguration sets both
    // synchronously) — so nothing has started yet. Ranked with `busy === 'remove'`
    // rather than last: both describe the removal the user just started, and a
    // standing precondition about Save must not out-rank the modal on screen.
    if (saving && busy === '') {
      return { reason: 'Disabled: confirm or dismiss the removal first', severity: 'info' }
    }
    if (!urlValid) {
      return { reason: 'Disabled: enter a valid TeamCity server URL first', severity: 'info' }
    }
    if (typesLoading) {
      return { reason: "Disabled: loading the server's jobs…", severity: 'info' }
    }
    if (!typesLoaded) {
      return {
        reason:
          'Disabled: click "Load available jobs" first — Canopy saves only jobs the server confirmed',
        severity: 'info',
      }
    }
    if (effectiveBuildTypes.length > CI_MAX_BUILD_TYPES) {
      return {
        reason: `Disabled: at most ${CI_MAX_BUILD_TYPES} jobs can be configured — untick ${effectiveBuildTypes.length - CI_MAX_BUILD_TYPES}`,
        severity: 'warn',
      }
    }
    if (effectiveBuildTypes.length === 0) {
      return { reason: 'Disabled: tick at least one job below', severity: 'info' }
    }
    if (busy === 'save') {
      return { reason: 'Disabled: an update is already in progress', severity: 'info' }
    }
    return { reason: '', severity: 'info' }
  })

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
      addToast('CI configuration saved — commit .canopy/config.json to share it')
      closeDialog()
    } catch (e) {
      saveError = e instanceof Error ? e.message : 'Failed to save CI configuration'
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
          'Removes the ci block from the git-tracked .canopy/config.json — after committing, the whole team loses the CI rows. Your stored token stays (Settings → CI connections).',
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
      saveError = e instanceof Error ? e.message : 'Failed to remove CI configuration'
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
        <h2 class="text-lg font-semibold text-text m-0 leading-tight">CI/CD — TeamCity</h2>
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
        <div role="status">
          {#if configLoadError}
            <p class="m-0 text-xs text-warning-text leading-snug" title={configLoadError}>
              <!-- The separator lives INSIDE each branch: the unknown-scope catch
                   path renders just the message, not a dangling em dash. -->
              {configLoadError}
              {#if configLoadScope === 'file'}
                — fix <code class="font-mono">.canopy/config.json</code> by hand; Save is disabled here
                because writing would require reading the file first (nothing is ever re-initialized over
                it).
              {:else if configLoadScope === 'block'}
                — pick the server and jobs below and Save to replace the invalid
                <code class="font-mono">ci</code> block — the rest of the file is untouched.
              {/if}
            </p>
          {/if}
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
              class="px-3 py-1 rounded-md text-sm font-inherit cursor-pointer border border-border bg-bg-input text-text-secondary hover:bg-hover-strong hover:text-text aria-disabled:opacity-50 aria-disabled:cursor-default aria-disabled:hover:bg-bg-input aria-disabled:hover:text-text-secondary"
              onclick={testConnection}
              aria-disabled={testing || !urlValid}
              aria-busy={testing}
              aria-describedby={serverBlockedReason ? 'ci-server-blocked' : undefined}
              title={testBlockedTitle ||
                'Check the connection against the server — nothing is saved'}
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

        <!-- NOT live: routine input changes do not need announcements;
             configLoadScope can also arrive asynchronously, but the config error
             is announced by the status region above. Reserved height keeps the row
             below stable; both buttons reference this on focus. -->
        <div class="min-h-4">
          {#if serverBlockedReason}
            <span id="ci-server-blocked" class="text-xs text-text-secondary break-words"
              >{serverBlockedReason}</span
            >
          {/if}
        </div>

        <!-- Persistent region: a load or keychain failure lands as a mutation —
             a span mounted together with its content is never announced. -->
        <div class="min-h-4" role="status">
          {#if typesError}
            <span class="text-xs text-danger-text">{typesError}</span>
          {/if}
        </div>

        <!-- Persistent live region for the modal's LIFETIME: a wrapper mounted in the
             same render pass as its content is skipped by screen readers, and every
             path that changes the message resets typesLoaded first — so the region
             must outlive the conditional chain below. -->
        <div role="status" id="ci-save-warnings">
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
                : ''}) — not TeamCity ids, so they cannot appear below. Correct them in
              <code class="font-mono">.canopy/config.json</code> to keep them; saving without doing so
              drops them.
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
            {@const shorten = (id: string): string => (id.length > 80 ? `${id.slice(0, 80)}…` : id)}
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
                  No longer on this server — saving drops them and there is nothing to re-tick:
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
                Save is disabled until you tick at least one job below — or use
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
                Tick at least one job below to save — the missing
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
            <CiJobPicker {serverTypes} {selected} onToggle={toggleType} onLabelChange={setLabel} />
          </div>
        {/if}
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
        <!-- Deliberately NOT live (the over-cap variant counts per tick — a live
             region would announce every checkbox); the button's aria-describedby
             reads it on focus, which is the modality that needs it. Renders
             ALONGSIDE saveError (the reference must never dangle — AT ignores a
             dangling describedby entirely), and in the same weight as the other
             blocking explanations: this is the only visible reason the primary
             button is dead, not a decorative hint. -->
        {#if saveBlockedState.reason}
          <span
            id="ci-save-blocked"
            class="text-xs break-words {saveBlockedState.severity === 'warn'
              ? 'text-warning-text'
              : 'text-text-secondary'}">{saveBlockedState.reason}</span
          >
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
            'Writes the ci block to .canopy/config.json — commit it to share with the team'}
          >{busy === 'save' ? 'Saving…' : 'Save configuration'}</button
        >
      </div>
    </footer>
  </div>
</div>
