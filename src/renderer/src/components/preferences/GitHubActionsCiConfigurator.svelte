<script lang="ts">
  import { onMount } from 'svelte'
  import { Check, LoaderCircle, Trash2, X } from '@lucide/svelte'
  import { SvelteMap } from 'svelte/reactivity'
  import { closeDialog, confirm } from '../../lib/stores/dialogs.svelte'
  import { workspaceState } from '../../lib/stores/workspace.svelte'
  import { addToast } from '../../lib/stores/toast.svelte'
  import { loadCiRepoConfig } from '../../lib/stores/ci.svelte'
  import { cycleFocus } from '../../lib/a11y/focusTrap'
  import { githubTokenCreationUrl } from '../../lib/ci/githubToken'
  import type { GitHubActionsCiRepoConfigInfo } from '../../lib/ci/types'
  import CiJobPicker from '../ci/CiJobPicker.svelte'
  import TrackerProviderIcon from '../shared/TrackerProviderIcon.svelte'
  import CredentialStorageNote from './_partials/CredentialStorageNote.svelte'

  interface DiscoveredWorkflow {
    id: string
    path: string
    name: string
    webUrl: string
    available: boolean
    error?: string
  }

  let repoRoot = $derived(workspaceState.selectedWorktreePath ?? workspaceState.repoRoot)
  let containerEl: HTMLElement | undefined = $state()
  let existingConfig = $state<GitHubActionsCiRepoConfigInfo | null>(null)
  let repository = $state('')
  let defaultBranch = $state('')
  let workflows = $state<DiscoveredWorkflow[]>([])
  let token = $state('')
  let hasToken = $state(false)
  let loading = $state(false)
  let testing = $state(false)
  let saving = $state(false)
  let testResult = $state<'success' | 'fail' | ''>('')
  let error = $state('')
  let loaded = $state(false)
  const selected = new SvelteMap<string, string>()

  let availableWorkflows = $derived(
    workflows
      .filter((workflow) => workflow.available)
      .map((workflow) => ({ id: workflow.path, name: workflow.name, projectName: defaultBranch })),
  )
  let unavailableWorkflows = $derived(workflows.filter((workflow) => !workflow.available))
  let missingConfiguredWorkflows = $derived(
    loaded
      ? (existingConfig?.workflows.filter(
          (configured) =>
            !workflows.some(
              (workflow) => workflow.path.toLowerCase() === configured.path.toLowerCase(),
            ),
        ) ?? [])
      : [],
  )
  let selectedWorkflows = $derived(
    availableWorkflows
      .filter((workflow) => selected.has(workflow.id))
      .map((workflow) => ({
        path: workflow.id,
        label: selected.get(workflow.id) || workflow.name,
      })),
  )
  let saveBlocked = $derived(
    saving || loading || !loaded || !repository || selectedWorkflows.length === 0,
  )
  let loadBlocked = $derived(loading || (!hasToken && token.trim().length === 0))

  onMount(async () => {
    containerEl?.focus()
    if (!repoRoot) return
    try {
      const [config, stored] = await Promise.all([
        window.api.ciConfig(repoRoot),
        window.api.keychainHasCredentials('github', 'https://github.com'),
      ])
      if (config.config?.provider === 'github-actions') {
        existingConfig = config.config
        repository = config.config.repository
        for (const workflow of config.config.workflows) {
          selected.set(workflow.path, workflow.label)
        }
      }
      if (config.invalid?.provider === 'github-actions') error = config.invalid.message
      hasToken = stored
      if (stored) void loadWorkflows()
    } catch (cause) {
      error = cause instanceof Error ? cause.message : 'Could not load GitHub Actions setup'
    }
  })

  function handleKeydown(event: KeyboardEvent): void {
    if (event.key === 'Escape') {
      event.preventDefault()
      if (!saving) closeDialog()
    } else if (event.key === 'Tab' && containerEl) {
      cycleFocus(containerEl, event)
    }
  }

  async function testConnection(): Promise<void> {
    const candidateToken = token.trim()
    if (!repoRoot || !candidateToken || testing || loading) return
    testing = true
    testResult = ''
    error = ''
    try {
      await window.api.ciTestGitHubConnection(repoRoot, candidateToken)
      testResult = 'success'
    } catch (cause) {
      testResult = 'fail'
      error = cause instanceof Error ? cause.message : 'GitHub connection failed'
    } finally {
      testing = false
    }
  }

  async function ensureToken(): Promise<boolean> {
    if (hasToken && !token) return true
    const candidateToken = token.trim()
    if (!repoRoot || !candidateToken) return false
    try {
      await window.api.ciTestGitHubConnection(repoRoot, candidateToken)
      await window.api.ciSetGitHubCredential(repoRoot, candidateToken)
      token = ''
      hasToken = true
      testResult = 'success'
      return true
    } catch (cause) {
      error = cause instanceof Error ? cause.message : 'Could not store the GitHub token'
      return false
    }
  }

  function openTokenPage(): void {
    void window.api.openExternal(githubTokenCreationUrl(repository))
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
      error = cause instanceof Error ? cause.message : 'Could not load GitHub workflows'
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
      error = cause instanceof Error ? cause.message : 'Could not save CI configuration'
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
      error = cause instanceof Error ? cause.message : 'Could not remove CI configuration'
    } finally {
      saving = false
    }
  }
</script>

<!-- svelte-ignore a11y_no_static_element_interactions -->
<div
  class="fixed inset-0 z-overlay flex justify-center items-center bg-scrim"
  onkeydown={handleKeydown}
  onmousedown={() => !saving && closeDialog()}
>
  <!-- svelte-ignore a11y_click_events_have_key_events -->
  <div
    bind:this={containerEl}
    class="outline-none w-[620px] max-w-[92vw] max-h-[85vh] flex flex-col bg-bg-overlay border border-border rounded-xl shadow-modal overflow-hidden"
    role="dialog"
    aria-modal="true"
    aria-labelledby="github-ci-title"
    tabindex="-1"
    onmousedown={(event) => event.stopPropagation()}
  >
    <header class="px-6 pt-5 pb-3 border-b border-border-subtle flex justify-between gap-3">
      <div class="min-w-0">
        <h2
          id="github-ci-title"
          class="m-0 text-lg font-semibold text-text flex items-center gap-2"
        >
          <TrackerProviderIcon provider="github" size={18} /> CI/CD — GitHub Actions
        </h2>
        <p class="m-0 mt-1 text-xs text-text-muted">
          Repository and workflows are shared via <code class="font-mono">.canopy/config.json</code
          >. The token stays on this machine.
        </p>
      </div>
      <button
        type="button"
        class="size-7 rounded-md border-0 bg-transparent text-text-muted hover:bg-hover hover:text-text aria-disabled:opacity-50"
        onclick={closeDialog}
        aria-label="Close"
        disabled={saving}><X size={16} /></button
      >
    </header>

    <div class="flex-1 overflow-y-auto px-6 py-5 flex flex-col gap-4">
      <div class="flex flex-col gap-1">
        <span class="text-2xs font-semibold uppercase tracking-caps-tight text-text-faint">
          GitHub repository
        </span>
        <div class="px-2.5 py-1.5 rounded-md border border-border bg-bg-input text-sm text-text">
          {repository || 'Resolved from this workspace’s origin remote'}
        </div>
        {#if defaultBranch}
          <span class="text-xs text-text-muted">Default branch: {defaultBranch}</span>
        {/if}
      </div>

      {#if !hasToken || token}
        <div class="flex flex-col gap-1">
          <div class="flex items-center justify-between gap-2">
            <label
              for="github-ci-token"
              class="text-2xs font-semibold uppercase tracking-caps-tight text-text-faint"
            >
              Personal access token
            </label>
            <button
              type="button"
              class="text-2xs text-accent-text bg-transparent border-0 p-0 cursor-pointer underline underline-offset-2 hover:text-accent"
              onclick={openTokenPage}
            >
              Generate token on GitHub →
            </button>
          </div>
          <input
            id="github-ci-token"
            type="password"
            class="px-2.5 py-1.5 border border-border rounded-md bg-bg-input text-text text-sm outline-none focus:border-focus-ring"
            bind:value={token}
            autocomplete="off"
            placeholder="Fine-grained token"
          />
          <p class="m-0 text-xs text-text-muted">
            GitHub opens with <strong>Actions — Read and write</strong> and
            <strong>Contents — Read-only</strong> preselected. Under Repository access choose
            <strong>Only select repositories</strong> and select
            <strong>{repository || 'this workspace repository'}</strong>. Workflow inputs are not
            secret fields.
          </p>
          <CredentialStorageNote
            provider="github"
            baseUrl="https://github.com"
            sharingNote={false}
          />
        </div>
      {/if}

      <div class="flex items-center gap-2">
        {#if token.trim()}
          <button
            type="button"
            class="px-3 py-1 rounded-md text-sm border border-border bg-bg-input text-text-secondary hover:bg-hover-strong aria-disabled:opacity-50"
            onclick={testConnection}
            disabled={testing || loading}
            aria-busy={testing}>{testing ? 'Testing…' : 'Test connection'}</button
          >
        {/if}
        <button
          type="button"
          class="px-3 py-1 rounded-md text-sm border border-border bg-bg-input text-text-secondary enabled:hover:bg-hover-strong disabled:opacity-50 disabled:cursor-not-allowed"
          onclick={loadWorkflows}
          disabled={loadBlocked}
          title={loadBlocked && !loading
            ? 'Add a GitHub token before loading workflows'
            : undefined}
          aria-busy={loading}>{loading ? 'Loading…' : 'Load workflows'}</button
        >
        <span class="text-xs" aria-live="polite">
          {#if testResult === 'success'}
            <span class="text-success flex items-center gap-1"><Check size={13} /> Connected</span>
          {:else if testResult === 'fail'}
            <span class="text-danger-text">Connection failed</span>
          {/if}
        </span>
      </div>

      <div class="min-h-5 text-xs text-danger-text break-words" role="status">{error}</div>

      {#if loading && !loaded}
        <div class="flex items-center gap-2 text-sm text-text-muted" role="status">
          <LoaderCircle size={14} class="animate-spin-slow motion-reduce:animate-none" />
          Loading dispatchable workflows…
        </div>
      {:else if loaded}
        {#if availableWorkflows.length > 0}
          <CiJobPicker
            serverTypes={availableWorkflows}
            {selected}
            onToggle={toggleWorkflow}
            onLabelChange={setLabel}
          />
        {:else}
          <p class="m-0 text-sm text-text-muted">
            No active workflows with <code class="font-mono">workflow_dispatch</code> were found.
          </p>
        {/if}
        {#if unavailableWorkflows.length > 0}
          <div class="flex flex-col gap-1">
            <span class="text-xs font-medium text-text-muted">Unavailable workflows</span>
            {#each unavailableWorkflows as workflow (workflow.path)}
              <div class="text-xs text-text-faint break-words">
                {workflow.name} — {workflow.error || 'not dispatchable'}
              </div>
            {/each}
          </div>
        {/if}
        {#if missingConfiguredWorkflows.length > 0}
          <div class="p-2 rounded-md bg-warning-bg text-xs text-warning-text" role="status">
            No longer returned by GitHub and removed on Save:
            {missingConfiguredWorkflows.map((workflow) => workflow.label).join(', ')}
          </div>
        {/if}
      {/if}
    </div>

    <footer class="px-6 py-3 border-t border-border-subtle flex items-center justify-between gap-3">
      <div>
        {#if existingConfig}
          <button
            type="button"
            class="flex items-center gap-1 px-2 py-1 border-0 bg-transparent text-xs text-text-faint hover:text-danger-text aria-disabled:opacity-50"
            onclick={removeConfiguration}
            disabled={saving}><Trash2 size={12} /> Remove CI configuration</button
          >
        {/if}
      </div>
      <div class="flex items-center gap-2">
        <button
          type="button"
          class="px-3 py-1 rounded-md text-sm border border-border bg-transparent text-text-secondary hover:bg-hover"
          onclick={closeDialog}
          disabled={saving}>Cancel</button
        >
        <button
          type="button"
          class="px-3 py-1 rounded-md text-sm border-0 bg-accent-bg text-accent-text hover:bg-accent-bg-hover aria-disabled:opacity-50"
          onclick={saveConfiguration}
          aria-disabled={saveBlocked}
          disabled={saveBlocked}
          title={saveBlocked
            ? 'Load and select at least one dispatchable workflow'
            : 'Save configuration'}>{saving ? 'Saving…' : 'Save configuration'}</button
        >
      </div>
    </footer>
  </div>
</div>
