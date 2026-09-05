<script lang="ts">
  import CredentialStorageNote from './CredentialStorageNote.svelte'
  import type { GitHubActionsCiConfiguratorState } from '../githubActionsCiConfiguratorState.svelte'

  let {
    state,
    class: className = '',
  }: { state: GitHubActionsCiConfiguratorState; class?: string } = $props()
</script>

<section class={`rounded-lg border border-border-subtle p-4 flex flex-col gap-3 ${className}`}>
  <div>
    <h3 class="m-0 text-sm font-semibold text-text">Personal credentials</h3>
    <p class="m-0 mt-0.5 text-xs text-text-muted leading-snug">
      Stored only on this machine and never written to
      <code class="font-mono">.canopy/config.json</code>.
    </p>
  </div>

  {#if state.repositoryReady && !state.isInitialSetup}
    <div
      class="flex items-center justify-between gap-3 rounded-md border border-border bg-bg-input px-2.5 py-2"
    >
      <div class="min-w-0">
        <div
          class="text-xs font-medium"
          class:text-danger-text={state.credentialRejected || !state.hasToken}
          class:text-text={state.hasToken && !state.credentialRejected}
        >
          {state.credentialRejected
            ? 'GitHub rejected the stored token'
            : state.hasToken
              ? 'GitHub Actions token stored'
              : 'No GitHub Actions token stored'}
        </div>
        <div class="truncate text-xs text-text-muted" title={state.credentialUrl}>
          {state.repository}
        </div>
      </div>
      <button
        bind:this={state.credentialButtonEl}
        type="button"
        class="shrink-0 px-2 py-1 rounded-md border border-border bg-transparent text-xs text-text-secondary cursor-pointer hover:bg-hover aria-disabled:opacity-50 aria-disabled:cursor-default aria-disabled:hover:bg-transparent"
        onclick={state.manageCredentials}
        aria-disabled={state.saving}
      >
        {state.hasToken ? 'Update token' : 'Add credentials'}
      </button>
    </div>
  {:else if state.repositoryReady}
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
          onclick={state.openTokenPage}
        >
          Generate token on GitHub →
        </button>
      </div>
      <input
        id="github-ci-token"
        type="password"
        class="px-2.5 py-1.5 border border-border rounded-md bg-bg-input text-text text-sm outline-none focus:border-focus-ring"
        bind:value={state.token}
        autocomplete="off"
        placeholder="Fine-grained token"
      />
      <p class="m-0 text-xs text-text-muted">
        Canopy asks GitHub to preselect <strong>Actions — Read and write</strong> and
        <strong>Contents — Read-only</strong>. Confirm both permissions and the expiry before
        generating. Under Repository access choose <strong>Only select repositories</strong> and
        select <strong>{state.repositoryLabel}</strong>. Workflow inputs are not secret fields.
      </p>
      <CredentialStorageNote
        provider="github-actions"
        baseUrl={state.credentialUrl}
        sharingNote={false}
      />
    </div>
  {:else if state.repositoryResolving}
    <p class="m-0 text-xs text-text-muted">
      Resolving this workspace’s <code class="font-mono">origin</code> remote…
    </p>
  {:else}
    <p class="m-0 text-xs text-text-muted">
      Resolve a supported <code class="font-mono">github.com</code> origin before creating or storing
      a GitHub Actions token.
    </p>
  {/if}

  {#if state.repositoryReady}
    <p class="m-0 text-xs text-text-muted">
      Git code transport is separate: fetch and push use the workspace’s
      <code class="font-mono">origin</code> through Git (SSH or its credential helper). This API
      token is bound only to GitHub Actions for <strong>{state.repositoryLabel}</strong> and does not
      grant Canopy Git push access.
    </p>
  {/if}
</section>
