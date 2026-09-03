<script lang="ts">
  import type { GitHubActionsCiConfiguratorState } from '../githubActionsCiConfiguratorState.svelte'

  let {
    state,
    class: className = '',
  }: { state: GitHubActionsCiConfiguratorState; class?: string } = $props()
</script>

<div class={`flex flex-col gap-1 ${className}`}>
  <span class="text-2xs font-semibold uppercase tracking-caps-tight text-text-faint">
    GitHub repository
  </span>
  <div class="px-2.5 py-1.5 rounded-md border border-border bg-bg-input text-sm text-text">
    {state.repository ||
      (state.repositoryResolving
        ? 'Resolving from this workspace’s origin remote…'
        : 'Unavailable')}
  </div>
  {#if state.defaultBranch}
    <span class="text-xs text-text-muted">Default branch: {state.defaultBranch}</span>
  {/if}
  <div role="status" class:sr-only={!state.rewritesSharedRepository}>
    {#if state.rewritesSharedRepository}
      <p class="m-0 text-xs leading-snug text-warning-text break-words">
        This workspace’s origin is <code class="font-mono">{state.repository}</code>, but the shared
        <code class="font-mono">ci</code> block names
        <code class="font-mono">{state.existingConfig?.repository}</code>. Saving rewrites it and
        causes a repository mismatch for anyone still using
        <code class="font-mono">{state.existingConfig?.repository}</code>. If this is a fork, close
        without saving.
      </p>
    {/if}
  </div>
</div>
