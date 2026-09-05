<script lang="ts">
  import { workspaceState } from '../../lib/stores/workspace.svelte'
  import CollapsibleSection from './CollapsibleSection.svelte'
  import PullRequestActions from './PullRequestActions.svelte'
</script>

<CollapsibleSection title="PULL REQUESTS" sectionKey="pullRequests" borderTop>
  {#snippet headerExtra()}
    <span class="text-2xs font-mono text-text-faint truncate" title={workspaceState.branch ?? ''}
      >{workspaceState.branch ?? ''}</span
    >
  {/snippet}
  <div class="flex flex-col">
    <PullRequestActions />
  </div>
</CollapsibleSection>
