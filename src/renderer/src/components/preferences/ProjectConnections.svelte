<script lang="ts">
  import { Check, Unlink, KeyRound, Settings, X } from '@lucide/svelte'
  import { confirm, showPreferences } from '../../lib/stores/dialogs.svelte'
  import {
    getRepoConfig,
    getTrackerCredentials,
    loadRepoConfig,
  } from '../../lib/stores/taskTracker.svelte'
  import { workspaceState } from '../../lib/stores/workspace.svelte'
  import { addToast } from '../../lib/stores/toast.svelte'
  import { providerLabel } from '../../lib/taskTracker/providerLabel'
  import PrefsSection from './_partials/PrefsSection.svelte'
  import TrackerEditForm from './_partials/TrackerEditForm.svelte'
  import CredentialStorageNote from './_partials/CredentialStorageNote.svelte'

  // Project connections = trackers configured in the repo's .canopy/config.json (active worktree).
  // Here you only CONNECT (authenticate) them — credentials are global per provider+URL. Adding /
  // editing / removing the tracker DEFINITION is intentionally not here (managed elsewhere).
  let repoRoot = $derived(workspaceState.selectedWorktreePath ?? workspaceState.repoRoot)
  let repoCfg = $derived(getRepoConfig())
  let trackers = $derived(repoCfg?.trackers ?? [])
  let trackerCreds = $derived(getTrackerCredentials())

  let connectingId = $state<string | null>(null)
  let formProvider = $state<'jira' | 'youtrack' | 'github'>('jira')
  let formBaseUrl = $state('')
  let formProjectKey = $state('')
  let formUsername = $state('')
  let formToken = $state('')
  let testing = $state(false)
  let testResult = $state<'success' | 'fail' | ''>('')
  let dialogEl = $state<HTMLElement>()

  let connectingCreds = $derived(connectingId ? trackerCreds[connectingId] : undefined)

  // Token entry happens in a focused dialog (not inline), so move focus there when it opens.
  $effect(() => {
    if (connectingId) dialogEl?.focus()
  })

  function startConnect(t: {
    id: string
    provider: string
    baseUrl: string
    projectKey?: string
  }): void {
    connectingId = t.id
    formProvider = t.provider as 'jira' | 'youtrack' | 'github'
    formBaseUrl = t.baseUrl
    formProjectKey = t.projectKey ?? ''
    formUsername = trackerCreds[t.id]?.username ?? ''
    formToken = ''
    testResult = ''
  }

  function cancel(): void {
    connectingId = null
    testResult = ''
  }

  function handleKeydown(e: KeyboardEvent): void {
    if (e.key === 'Escape') {
      e.preventDefault()
      cancel()
    }
  }

  async function testConnection(): Promise<void> {
    testing = true
    testResult = ''
    try {
      await window.api.taskTrackerTestNewConnection({
        provider: formProvider,
        name: `${formProvider}:${formBaseUrl}`,
        baseUrl: formBaseUrl.replace(/\/$/, ''),
        projectKey: formProjectKey || undefined,
        username: formUsername || undefined,
        token: formToken,
      })
      testResult = 'success'
    } catch {
      testResult = 'fail'
    } finally {
      testing = false
    }
  }

  // The dialog itself (explicit "Save credentials" button + storage note) is the deliberate step,
  // so there's no extra confirm here.
  async function saveCredentials(): Promise<void> {
    const url = formBaseUrl.replace(/\/$/, '')
    try {
      await window.api.keychainSetCredentials(
        formProvider,
        url,
        formToken,
        formUsername || undefined,
      )
    } catch (e) {
      addToast(e instanceof Error ? e.message : 'Failed to save credentials')
      return
    }
    if (repoRoot) await loadRepoConfig(repoRoot)
    connectingId = null
    addToast('Credentials saved')
  }

  async function removeCredentials(t: { provider: string; baseUrl: string }): Promise<void> {
    const ok = await confirm({
      title: 'Remove credentials',
      message: `Remove your stored token for ${providerLabel(t.provider)} at ${t.baseUrl}?`,
      details:
        'Clears the token on this machine only. The token is global (keyed by provider + URL), so this affects every connection using this URL across all your projects. The tracker stays configured in the repo.',
      confirmLabel: 'Remove credentials',
      destructive: true,
    })
    if (!ok) return
    try {
      await window.api.keychainDeleteCredentials(t.provider, t.baseUrl)
    } catch (e) {
      addToast(e instanceof Error ? e.message : 'Failed to remove credentials')
      return
    }
    if (repoRoot) await loadRepoConfig(repoRoot)
    addToast('Credentials removed')
  }
</script>

<PrefsSection
  title="Connections"
  description="Connect this project's trackers with your credentials"
>
  <div class="flex flex-col gap-2">
    {#if !repoRoot}
      <p class="text-sm text-text-faint m-0">Open a repository to connect its trackers.</p>
    {:else if trackers.length === 0}
      <p class="text-sm text-text-faint m-0">
        No tracker is configured in this project's
        <code class="font-mono text-text-secondary">.canopy/config.json</code>.
      </p>
    {/if}

    {#each trackers as tracker (tracker.id)}
      {@const creds = trackerCreds[tracker.id]}
      <div class="flex items-center gap-1">
        <div
          class="flex-1 flex items-center gap-2 px-2.5 py-1.5 border border-border-subtle rounded-md bg-bg-input text-text text-sm min-w-0"
        >
          <span
            class="text-2xs font-semibold uppercase tracking-caps-tight text-accent-text bg-accent-bg px-1.5 py-px rounded-sm shrink-0"
            >{providerLabel(tracker.provider)}</span
          >
          <span
            class="flex-1 text-text-secondary truncate"
            title={tracker.baseUrl || 'Not configured'}>{tracker.baseUrl || 'Not configured'}</span
          >
          {#if tracker.projectKey}
            <span class="font-mono text-xs text-text-muted shrink-0">{tracker.projectKey}</span>
          {/if}
          {#if creds?.hasToken && creds.valid === false}
            <span
              class="text-2xs text-warning-text shrink-0"
              title="The stored token was rejected by the tracker — it may have expired or been revoked"
              >Credentials expired</span
            >
          {:else if creds?.hasToken}
            <span
              class="flex items-center gap-1 text-2xs text-success shrink-0"
              title={creds.username ? `Connected (${creds.username})` : 'Connected'}
            >
              <Check size={12} />
              {#if creds.username}<span class="text-text-muted max-w-24 truncate"
                  >{creds.username}</span
                >{/if}
            </span>
          {:else}
            <span class="text-2xs text-warning-text shrink-0">No credentials</span>
          {/if}
        </div>
        {#if creds?.hasToken}
          {#if creds.valid === false}
            <button
              type="button"
              class="flex items-center gap-1 px-2.5 py-1 rounded-md bg-accent-bg border-0 text-accent-text text-xs font-inherit cursor-pointer hover:bg-accent-bg-hover"
              onclick={() => startConnect(tracker)}
              title="Enter a new token — the stored one no longer works"
            >
              <KeyRound size={12} />
              Reconnect
            </button>
          {:else}
            <button
              type="button"
              class="flex items-center gap-1 px-2 py-1 rounded-md bg-transparent border border-border text-text-secondary text-xs font-inherit cursor-pointer hover:bg-hover hover:text-text"
              onclick={() => startConnect(tracker)}
              title="Change the stored token for this tracker">Change</button
            >
          {/if}
          <button
            type="button"
            class="flex items-center justify-center size-7 rounded-md bg-transparent border-0 text-text-muted cursor-pointer hover:bg-hover hover:text-text"
            onclick={() => removeCredentials(tracker)}
            aria-label="Remove credentials"
            title={`Remove the credentials for ${providerLabel(tracker.provider)} at ${tracker.baseUrl} — affects every project that connects to this URL`}
          >
            <Unlink size={12} />
          </button>
        {:else}
          <button
            type="button"
            class="flex items-center gap-1 px-2.5 py-1 rounded-md bg-accent-bg border-0 text-accent-text text-xs font-inherit cursor-pointer hover:bg-accent-bg-hover"
            onclick={() => startConnect(tracker)}
            title="Enter your credentials to connect this tracker"
          >
            <KeyRound size={12} />
            Connect
          </button>
        {/if}
      </div>
    {/each}

    <button
      type="button"
      class="self-start flex items-center gap-1 px-1.5 py-0.5 border-0 bg-transparent text-text-faint text-xs font-inherit cursor-pointer hover:text-text-secondary"
      onclick={() => showPreferences('Your connections')}
      title="Manage all your credentials and connections in Settings"
    >
      <Settings size={12} />
      <span>Manage all credentials in Settings</span>
    </button>
  </div>
</PrefsSection>

{#if connectingId}
  <!-- Token entry dialog, layered above the Project tracker modal. -->
  <!-- svelte-ignore a11y_no_static_element_interactions -->
  <!-- svelte-ignore a11y_click_events_have_key_events -->
  <div
    class="fixed inset-0 z-[10010] flex justify-center items-center bg-scrim"
    onmousedown={cancel}
    onkeydown={handleKeydown}
  >
    <div
      bind:this={dialogEl}
      class="outline-none w-[460px] max-w-[92vw] max-h-[85vh] overflow-y-auto flex flex-col gap-3 bg-bg-overlay border border-border rounded-xl shadow-modal p-5"
      role="dialog"
      aria-modal="true"
      aria-label="Connect tracker"
      tabindex="-1"
      onmousedown={(e) => e.stopPropagation()}
    >
      <header class="flex items-start justify-between gap-3">
        <div class="flex flex-col gap-0.5 min-w-0">
          <h3 class="text-base font-semibold text-text m-0 leading-tight">
            {connectingCreds?.hasToken ? 'Update credentials' : 'Connect'} — {providerLabel(
              formProvider,
            )}
          </h3>
          <p class="text-xs text-text-muted m-0 truncate" title={formBaseUrl}>{formBaseUrl}</p>
        </div>
        <button
          type="button"
          class="flex items-center justify-center size-7 rounded-md bg-transparent border-0 text-text-muted cursor-pointer hover:bg-hover hover:text-text shrink-0"
          onclick={cancel}
          aria-label="Close"
          title="Close"
        >
          <X size={16} />
        </button>
      </header>

      <CredentialStorageNote provider={formProvider} baseUrl={formBaseUrl.replace(/\/$/, '')} />

      <TrackerEditForm
        bind:provider={formProvider}
        bind:baseUrl={formBaseUrl}
        bind:projectKey={formProjectKey}
        bind:username={formUsername}
        bind:token={formToken}
        isNew={false}
        hasExistingToken={connectingCreds?.hasToken ?? false}
        credentialsOnly={true}
        {testing}
        {testResult}
        onCancel={cancel}
        onTest={testConnection}
        onSave={saveCredentials}
      />
    </div>
  </div>
{/if}
