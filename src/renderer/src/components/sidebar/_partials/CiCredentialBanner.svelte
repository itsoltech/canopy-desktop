<script lang="ts">
  import { KeyRound } from '@lucide/svelte'
  import { formatDateTime } from '../../../lib/formatDate'
  import { credentialConfiguratorSection } from '../../../lib/ci/credentialGate'
  import type { CiSectionState } from '../ciSectionState.svelte'

  let { state, class: className = '' }: { state: CiSectionState; class?: string } = $props()
</script>

<div class={`px-2 py-1 ${className}`}>
  <div
    class="flex items-center gap-2 rounded-lg border border-experimental-border bg-experimental-bg px-3 py-2"
    title={state.credentialApprovalRequired
      ? `Approve ${state.providerLabel} credentials for this repository`
      : state.credentialsRejected
        ? `${state.providerLabel} credentials need updating${state.rejectedSince ? ` since ${formatDateTime(Date.parse(state.rejectedSince))}` : ''}`
        : state.providerUrl}
  >
    <KeyRound size={13} class="shrink-0 text-warning-text" />
    <span class="flex-1 min-w-0 text-xs text-text-secondary leading-snug">
      {#if state.credentialApprovalRequired}
        Approve the stored TeamCity token for this repository and its selected jobs.
      {:else if state.credentialsRejected}
        {state.providerLabel} rejected the stored token.{state.rejectedSince
          ? ` Since ${formatDateTime(Date.parse(state.rejectedSince))}.`
          : ''}
      {:else}
        {state.provider === 'github-actions'
          ? 'No token for this GitHub repository.'
          : 'No token for this CI server.'}
      {/if}
    </span>
    <button
      type="button"
      class="shrink-0 px-2 py-0.5 rounded-md border border-border bg-transparent text-xs text-text-secondary font-inherit cursor-pointer hover:border-accent-muted hover:text-accent-text"
      onclick={() =>
        state.openConfigurator(credentialConfiguratorSection(state.credentialApprovalRequired))}
    >
      {state.credentialApprovalRequired
        ? 'Review access'
        : state.credentialsRejected
          ? 'Update token'
          : 'Add credentials'}
    </button>
  </div>
</div>
