<script lang="ts">
  import { History, LoaderCircle } from '@lucide/svelte'
  import CiLastJobCard from '../../ci/CiLastJobCard.svelte'
  import CiLastRunCard from '../../ci/CiLastRunCard.svelte'
  import type { CiSectionState } from '../ciSectionState.svelte'

  let { state, class: className = '' }: { state: CiSectionState; class?: string } = $props()
</script>

<div class={className}>
  {#if state.branchLoading || !state.activityLoaded}
    <div class="flex items-center gap-2.5 w-full h-7 px-3 text-sm text-text-faint">
      <LoaderCircle size={13} class="animate-spin-slow shrink-0 motion-reduce:animate-none" />
      <span>Loading jobs history…</span>
    </div>
  {:else if state.hasCardRows && state.branch}
    {#if state.provider === 'github-actions'}
      <CiLastRunCard
        rows={state.jobRows}
        branch={state.branch}
        issue={state.activityIssue}
        onActivate={state.openBranchActivity}
      />
    {:else}
      <CiLastJobCard
        rows={state.branchRows}
        branch={state.branch}
        issue={state.activityIssue}
        onActivate={state.openBranchActivity}
      />
    {/if}
  {:else}
    <button
      class="group flex items-center gap-2.5 w-full h-7 px-3 border-0 bg-transparent text-text text-sm font-inherit cursor-pointer text-left transition-colors duration-fast hover:bg-hover"
      onclick={state.openActivity}
      aria-haspopup="dialog"
      title={`Recent and running jobs for this repository — opens in a window${
        state.activityIssue ? ` (${state.activityIssue.detail})` : ''
      }`}
    >
      <History
        size={13}
        class="text-text-faint group-hover:text-text-secondary group-focus-within:text-text-secondary shrink-0"
      />
      <span class="flex-1">Jobs history</span>
      {#if state.activityIssue}
        <span
          class="px-1.5 py-px rounded-md text-2xs shrink-0 bg-warning-bg text-warning-text"
          title={state.activityIssue.detail}
        >
          {state.activityIssue.label}
        </span>
      {:else if !state.activityLoaded}
        <LoaderCircle
          size={12}
          class="text-text-faint animate-spin-slow shrink-0 motion-reduce:animate-none"
        />
      {/if}
    </button>

    {#if state.branchError}
      <div class="px-3 py-1 text-xs text-warning-text truncate" title={state.branchError}>
        Last job unavailable — {state.branchError}
      </div>
    {/if}
  {/if}
</div>
