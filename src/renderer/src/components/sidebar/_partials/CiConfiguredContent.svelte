<script lang="ts">
  import { ExternalLink, Play } from '@lucide/svelte'
  import TrackerProviderIcon from '../../shared/TrackerProviderIcon.svelte'
  import type { CiSectionState } from '../ciSectionState.svelte'
  import CiActivityEntry from './CiActivityEntry.svelte'
  import CiCredentialBanner from './CiCredentialBanner.svelte'

  let { state, class: className = '' }: { state: CiSectionState; class?: string } = $props()
</script>

<div class={`flex flex-col ${className}`}>
  <button
    class="group flex items-center gap-2.5 w-full h-7 pl-3 pr-1 border-0 bg-transparent text-text text-sm font-inherit cursor-pointer text-left transition-colors duration-fast enabled:hover:bg-hover"
    onclick={state.openProvider}
    title={state.provider === 'github-actions'
      ? 'Open repository in GitHub'
      : 'Open TeamCity in the browser'}
  >
    <span class="inline-flex items-center flex-shrink-0">
      <TrackerProviderIcon
        provider={state.provider === 'github-actions' ? 'github' : 'teamcity'}
        size={13}
      />
    </span>
    <span class="overflow-hidden text-ellipsis whitespace-nowrap flex-1" title={state.providerUrl}>
      {state.config?.provider === 'github-actions'
        ? state.config.repository
        : state.config?.baseUrl}
    </span>
    <ExternalLink
      size={11}
      class="shrink-0 opacity-0 transition-opacity duration-fast group-hover:opacity-60 group-focus-within:opacity-60"
    />
  </button>

  {#if !state.cfgState.hasToken || state.credentialsRejected || state.credentialApprovalRequired}
    <CiCredentialBanner {state} />
  {:else}
    <button
      class="group flex items-center gap-2.5 w-full h-7 px-3 border-0 bg-transparent text-text text-sm font-inherit cursor-pointer text-left transition-colors duration-fast enabled:hover:bg-hover"
      onclick={state.openRunJob}
      title={state.runActionTitle}
    >
      <Play
        size={13}
        class="text-text-faint group-enabled:group-hover:text-text-secondary group-focus-within:text-text-secondary flex-shrink-0"
      />
      <span class="flex-1">{state.runActionLabel}</span>
    </button>

    <div
      class="h-px mx-3 my-1 bg-border-subtle"
      role="separator"
      aria-orientation="horizontal"
    ></div>
    <CiActivityEntry {state} />
  {/if}
</div>
