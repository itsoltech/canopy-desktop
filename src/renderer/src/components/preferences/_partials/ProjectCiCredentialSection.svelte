<script lang="ts">
  import CredentialStorageNote from './CredentialStorageNote.svelte'
  import type { ProjectCiModalState } from '../projectCiModalState.svelte'

  let { state, class: className = '' }: { state: ProjectCiModalState; class?: string } = $props()
</script>

<section class={`rounded-lg border border-border-subtle p-4 flex flex-col gap-3 ${className}`}>
  <div>
    <h3 class="m-0 text-sm font-semibold text-text">Personal credentials</h3>
    <p class="m-0 mt-0.5 text-xs text-text-muted leading-snug">
      Stored only on this machine and never written to
      <code class="font-mono">.canopy/config.json</code>.
    </p>
  </div>

  {#if !state.isInitialSetup}
    <div
      class="flex items-center justify-between gap-3 rounded-md border border-border bg-bg-input px-2.5 py-2"
    >
      <div class="min-w-0">
        <div
          class="text-xs font-medium"
          class:text-danger-text={!state.serverHasToken || state.credentialRejected}
          class:text-text={state.serverHasToken && !state.credentialRejected}
        >
          {state.credentialGate.credentialLabel}
        </div>
        <div class="truncate text-xs text-text-muted" title={state.effectiveUrl}>
          {state.effectiveUrl}
        </div>
      </div>
      <button
        bind:this={state.credentialButtonEl}
        type="button"
        class="shrink-0 px-2 py-1 rounded-md border border-border bg-transparent text-xs text-text-secondary cursor-pointer hover:bg-hover"
        onclick={state.manageCredentials}
        aria-disabled={state.busy !== ''}
      >
        {state.selectedServer === state.existingConfig?.baseUrl
          ? state.serverHasToken
            ? 'Update token'
            : 'Add credentials'
          : 'Manage credentials'}
      </button>
    </div>
  {:else if !state.serverHasToken || state.credentialRejected || state.selectedServer === state.newServerValue}
    <div class="flex flex-col gap-1">
      <span class="text-2xs font-semibold uppercase tracking-caps-tight text-text-faint">
        Access token
      </span>
      <input
        class="px-2.5 py-1.5 border border-border rounded-md bg-bg-input text-text text-sm font-inherit outline-none focus:border-focus-ring placeholder:text-text-faint"
        type="password"
        name="ciModalToken"
        aria-label="TeamCity access token"
        bind:value={state.formToken}
        placeholder="Enter token"
        autocomplete="off"
        title="Stored for this server-scoped CI integration on your machine - never written to your repository"
      />
      <div class="mt-1">
        <CredentialStorageNote
          provider="teamcity"
          baseUrl={state.urlValid ? state.effectiveUrl : undefined}
          sharingNote={false}
        />
      </div>
    </div>
  {:else}
    <div class="flex items-center gap-3 rounded-md border border-border bg-bg-input px-2.5 py-2">
      <div class="min-w-0">
        <div
          class="text-xs font-medium"
          class:text-danger-text={state.credentialRejected}
          class:text-text={!state.credentialRejected}
        >
          {state.credentialGate.credentialLabel}
        </div>
        <div class="truncate text-xs text-text-muted" title={state.effectiveUrl}>
          {state.effectiveUrl}
        </div>
      </div>
    </div>
  {/if}
</section>
