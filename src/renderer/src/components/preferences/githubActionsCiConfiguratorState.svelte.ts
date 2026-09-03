import { onMount, tick } from 'svelte'
import { SvelteMap } from 'svelte/reactivity'
import { closeDialog, confirm } from '../../lib/stores/dialogs.svelte'
import { addToast } from '../../lib/stores/toast.svelte'
import { bumpCiCredentialTick, loadCiRepoConfig } from '../../lib/stores/ci.svelte'
import { cycleFocus } from '../../lib/a11y/focusTrap'
import { githubTokenCreationUrl } from '../../lib/ci/githubToken'
import { ipcErrorMessage } from '../../lib/ci/errors'
import type { GitHubActionsCiRepoConfigInfo } from '../../lib/ci/types'
import { githubActionsCredentialBaseUrl } from '../../../../renderer-shared/credentialBindings'
import { CI_MAX_WORKFLOWS, ciWorkflowSelectionOverflow } from '../../lib/ci/limits'

export interface InvalidCiConfig {
  scope: 'file' | 'block'
  message: string
  provider?: 'teamcity' | 'github-actions'
}

export interface DiscoveredWorkflow {
  id: string
  path: string
  name: string
  webUrl: string
  available: boolean
  error?: string
}

// Keep this factory inferred so the exported ReturnType stays aligned with its reactive getters.
// eslint-disable-next-line @typescript-eslint/explicit-function-return-type
export function createGitHubActionsCiConfiguratorState({
  repoRoot,
  initialConfig,
  initialInvalid,
}: {
  repoRoot: string
  initialConfig: GitHubActionsCiRepoConfigInfo | null
  initialInvalid?: InvalidCiConfig
}) {
  let containerEl: HTMLElement | undefined = $state()
  let existingConfig = $state<GitHubActionsCiRepoConfigInfo | null>(null)
  let repository = $state('')
  let repositoryResolving = $state(true)
  let repositoryResolutionIssue = $state('')
  let defaultBranch = $state('')
  let workflows = $state<DiscoveredWorkflow[]>([])
  let token = $state('')
  let hasToken = $state(false)
  let credentialRejected = $state(false)
  let loading = $state(false)
  let testing = $state(false)
  let saving = $state(false)
  let testResult = $state<'success' | 'fail' | ''>('')
  let error = $state('')
  let loaded = $state(false)
  let credentialEditorOpen = $state(false)
  let credentialButtonEl: HTMLButtonElement | undefined = $state()
  const selected = new SvelteMap<string, string>()

  const availableWorkflows = $derived(
    workflows
      .filter((workflow) => workflow.available)
      .map((workflow) => ({ id: workflow.path, name: workflow.name, projectName: defaultBranch })),
  )
  const unavailableWorkflows = $derived(workflows.filter((workflow) => !workflow.available))
  const missingConfiguredWorkflows = $derived(
    loaded
      ? (existingConfig?.workflows.filter(
          (configured) =>
            !workflows.some(
              (workflow) => workflow.path.toLowerCase() === configured.path.toLowerCase(),
            ),
        ) ?? [])
      : [],
  )
  const selectedWorkflows = $derived(
    availableWorkflows
      .filter((workflow) => selected.has(workflow.id))
      .map((workflow) => ({
        path: workflow.id,
        label: selected.get(workflow.id) || workflow.name,
      })),
  )
  const workflowSelectionOverflow = $derived(ciWorkflowSelectionOverflow(selectedWorkflows.length))
  const saveBlocked = $derived(
    saving ||
      loading ||
      !loaded ||
      !repository ||
      selectedWorkflows.length === 0 ||
      workflowSelectionOverflow > 0,
  )
  const saveBlockedReason = $derived(
    saving
      ? 'An update is already in progress'
      : loading
        ? 'Workflows are still loading'
        : !loaded
          ? 'Load workflows before saving'
          : workflowSelectionOverflow > 0
            ? `At most ${CI_MAX_WORKFLOWS} workflows can be configured - untick ${workflowSelectionOverflow}`
            : selectedWorkflows.length === 0
              ? 'Select at least one dispatchable workflow'
              : !repository
                ? 'The GitHub repository is unavailable'
                : '',
  )
  const loadBlocked = $derived(
    loading ||
      repositoryResolving ||
      !!repositoryResolutionIssue ||
      !repository ||
      (hasToken && credentialRejected) ||
      (!hasToken && token.trim().length === 0),
  )
  const loadBlockedReason = $derived(
    loading || repositoryResolving
      ? ''
      : repositoryResolutionIssue
        ? repositoryResolutionIssue
        : !repository
          ? 'No github.com origin remote was found for this workspace.'
          : hasToken && credentialRejected
            ? 'Update the rejected token in Personal credentials before loading workflows.'
            : !hasToken && token.trim().length === 0
              ? isInitialSetup
                ? 'Add a GitHub token before loading workflows.'
                : 'Add a token in Personal credentials before loading workflows.'
              : '',
  )
  const repositoryLabel = $derived(repository || 'this workspace repository')
  const repositoryReady = $derived(
    !repositoryResolving && !repositoryResolutionIssue && !!repository,
  )
  const rewritesSharedRepository = $derived(
    !!existingConfig &&
      !!repository &&
      existingConfig.repository.toLowerCase() !== repository.toLowerCase(),
  )
  const credentialUrl = $derived(repository ? githubActionsCredentialBaseUrl(repository) : '')
  const isInitialSetup = $derived(initialConfig === null)

  onMount(async () => {
    containerEl?.focus()
    if (!repoRoot) {
      repositoryResolving = false
      repositoryResolutionIssue = 'No workspace is available for GitHub Actions setup.'
      return
    }
    if (initialConfig) {
      existingConfig = initialConfig
      repository = initialConfig.repository
      for (const workflow of initialConfig.workflows) {
        selected.set(workflow.path, workflow.label)
      }
    }
    if (initialInvalid?.provider === 'github-actions') error = initialInvalid.message
    let loadStoredConfiguration = false
    try {
      const lookup = await window.api.githubGetRepoIdentifier(repoRoot)
      if (lookup.status === 'missing') {
        repository = ''
        repositoryResolutionIssue = 'No github.com origin remote was found for this workspace.'
      } else if (lookup.status === 'error') {
        repository = ''
        repositoryResolutionIssue = `Could not resolve this workspace’s origin remote: ${lookup.message}`
      } else if (lookup.identifier.host.toLowerCase() !== 'github.com') {
        repository = ''
        repositoryResolutionIssue = `GitHub Actions currently supports github.com origins only; this workspace uses ${lookup.identifier.host}.`
      } else {
        const { identifier } = lookup
        // Setup and credentials follow the local origin. adapterForConfig later
        // requires the saved value to equal origin, so a rewrite is warned below.
        repository = `${identifier.owner}/${identifier.repo}`.toLowerCase()
        repositoryResolutionIssue = ''
      }
      try {
        const storedCredential = credentialUrl
          ? await window.api.keychainGetCredentials('github-actions', credentialUrl)
          : null
        hasToken = storedCredential?.hasToken ?? false
        credentialRejected = storedCredential?.authenticationState === 'invalid'
      } catch (cause) {
        hasToken = false
        credentialRejected = false
        error = ipcErrorMessage(cause, 'Could not check the stored GitHub token')
      }
      loadStoredConfiguration = hasToken && !credentialRejected
    } catch (cause) {
      repository = ''
      error = ipcErrorMessage(cause, 'Could not load GitHub Actions setup')
      repositoryResolutionIssue = 'Could not resolve this workspace’s GitHub origin remote.'
    } finally {
      repositoryResolving = false
    }
    if (loadStoredConfiguration) void loadWorkflows()
  })

  function handleKeydown(event: KeyboardEvent): void {
    if (event.key === 'Escape') {
      event.preventDefault()
      requestClose()
    } else if (event.key === 'Tab' && containerEl) {
      cycleFocus(containerEl, event)
    }
  }

  function requestClose(): void {
    if (saving) return
    closeDialog()
  }

  async function testConnection(): Promise<void> {
    const candidateToken = token.trim()
    if (!repoRoot || !repositoryReady || !candidateToken || testing || loading) return
    testing = true
    testResult = ''
    error = ''
    try {
      await window.api.ciTestGitHubConnection(repoRoot, candidateToken)
      testResult = 'success'
    } catch (cause) {
      testResult = 'fail'
      error = ipcErrorMessage(cause, 'GitHub connection failed')
    } finally {
      testing = false
    }
  }

  async function ensureToken(): Promise<boolean> {
    if (!repositoryReady) return false
    if (hasToken && token.trim().length === 0) return !credentialRejected
    const candidateToken = token.trim()
    if (!repoRoot || !candidateToken) return false
    try {
      await window.api.ciTestGitHubConnection(repoRoot, candidateToken)
      await window.api.ciSetGitHubCredential(repoRoot, candidateToken)
      bumpCiCredentialTick()
      token = ''
      hasToken = true
      credentialRejected = false
      testResult = 'success'
      return true
    } catch (cause) {
      error = ipcErrorMessage(cause, 'Could not store the GitHub token')
      return false
    }
  }

  function openTokenPage(): void {
    if (!repositoryReady) return
    void window.api.openExternal(githubTokenCreationUrl(repository))
  }

  function manageCredentials(): void {
    if (saving) return
    credentialEditorOpen = true
  }

  function credentialUpdated(): void {
    hasToken = true
    credentialRejected = false
    error = ''
  }

  async function closeCredentialEditor(): Promise<void> {
    credentialEditorOpen = false
    await tick()
    credentialButtonEl?.focus()
  }

  async function loadWorkflows(): Promise<void> {
    if (!repoRoot || loadBlocked) return
    loading = true
    error = ''
    try {
      if (!(await ensureToken())) {
        if (!error) error = 'Enter a GitHub token before loading workflows.'
        loaded = false
        return
      }
      const setup = await window.api.ciGitHubSetup(repoRoot)
      repository = setup.repository
      defaultBranch = setup.defaultBranch
      workflows = setup.workflows
      loaded = true
    } catch (cause) {
      const message = ipcErrorMessage(cause, 'Could not load GitHub workflows')
      const storedCredential =
        hasToken && token.trim().length === 0 && credentialUrl
          ? await window.api
              .keychainGetCredentials('github-actions', credentialUrl)
              .catch(() => null)
          : null
      credentialRejected = storedCredential?.authenticationState === 'invalid'
      error = credentialRejected
        ? ''
        : hasToken && token.trim().length === 0
          ? `${message}. Replace the stored token if it cannot access ${repository}.`
          : message
      loaded = false
    } finally {
      loading = false
    }
  }

  function toggleWorkflow(workflow: { id: string; name: string }): void {
    if (selected.has(workflow.id)) selected.delete(workflow.id)
    else selected.set(workflow.id, workflow.name)
    error = ''
  }

  function setLabel(path: string, label: string): void {
    selected.set(path, label)
    error = ''
  }

  async function saveConfiguration(): Promise<void> {
    if (!repoRoot || saveBlocked) return
    saving = true
    error = ''
    try {
      await window.api.ciSaveConfig(repoRoot, {
        provider: 'github-actions',
        baseUrl: 'https://github.com',
        repository,
        workflows: selectedWorkflows,
      })
      await loadCiRepoConfig(repoRoot)
      addToast('GitHub Actions configuration saved — commit .canopy/config.json to share it')
      closeDialog()
    } catch (cause) {
      error = ipcErrorMessage(cause, 'Could not save CI configuration')
    } finally {
      saving = false
    }
  }

  async function removeConfiguration(): Promise<void> {
    if (!repoRoot || !existingConfig || saving) return
    const accepted = await confirm({
      title: 'Remove CI configuration',
      message: `Remove GitHub Actions (${existingConfig.repository}) from this repository?`,
      details: 'This removes the shared ci block. Your personal GitHub token remains in Settings.',
      confirmLabel: 'Remove configuration',
      destructive: true,
    })
    if (!accepted) return
    saving = true
    try {
      await window.api.ciSaveConfig(repoRoot, null)
      await loadCiRepoConfig(repoRoot)
      addToast('CI configuration removed')
      closeDialog()
    } catch (cause) {
      error = ipcErrorMessage(cause, 'Could not remove CI configuration')
    } finally {
      saving = false
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
    get repository() {
      return repository
    },
    get repositoryResolving() {
      return repositoryResolving
    },
    get repositoryResolutionIssue() {
      return repositoryResolutionIssue
    },
    get defaultBranch() {
      return defaultBranch
    },
    get workflows() {
      return workflows
    },
    get token() {
      return token
    },
    set token(value: string) {
      token = value
    },
    get hasToken() {
      return hasToken
    },
    get credentialRejected() {
      return credentialRejected
    },
    get loading() {
      return loading
    },
    get testing() {
      return testing
    },
    get saving() {
      return saving
    },
    get testResult() {
      return testResult
    },
    get error() {
      return error
    },
    get loaded() {
      return loaded
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
    selected,
    get availableWorkflows() {
      return availableWorkflows
    },
    get unavailableWorkflows() {
      return unavailableWorkflows
    },
    get missingConfiguredWorkflows() {
      return missingConfiguredWorkflows
    },
    get selectedWorkflows() {
      return selectedWorkflows
    },
    get workflowSelectionOverflow() {
      return workflowSelectionOverflow
    },
    get saveBlocked() {
      return saveBlocked
    },
    get saveBlockedReason() {
      return saveBlockedReason
    },
    get loadBlocked() {
      return loadBlocked
    },
    get loadBlockedReason() {
      return loadBlockedReason
    },
    get repositoryLabel() {
      return repositoryLabel
    },
    get repositoryReady() {
      return repositoryReady
    },
    get rewritesSharedRepository() {
      return rewritesSharedRepository
    },
    get credentialUrl() {
      return credentialUrl
    },
    get isInitialSetup() {
      return isInitialSetup
    },
    handleKeydown,
    requestClose,
    testConnection,
    openTokenPage,
    manageCredentials,
    credentialUpdated,
    closeCredentialEditor,
    loadWorkflows,
    toggleWorkflow,
    setLabel,
    saveConfiguration,
    removeConfiguration,
  }
}

export type GitHubActionsCiConfiguratorState = ReturnType<
  typeof createGitHubActionsCiConfiguratorState
>
