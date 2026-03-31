<script lang="ts">
  import { onMount } from 'svelte'
  import { SquareKanban, Plus, ExternalLink } from '@lucide/svelte'
  import CollapsibleSection from './CollapsibleSection.svelte'
  import {
    getIssueTrackerConnections,
    isIssueTrackerLoading,
    loadConnections,
  } from '../../lib/stores/issueTracker.svelte'
  import { showPreferences } from '../../lib/stores/dialogs.svelte'
  import { showIssuePicker } from '../../lib/stores/dialogs.svelte'

  onMount(() => {
    loadConnections()
  })

  let connections = $derived(getIssueTrackerConnections())
  let loading = $derived(isIssueTrackerLoading())

  function providerLabel(provider: string): string {
    if (provider === 'jira') return 'Jira'
    if (provider === 'youtrack') return 'YouTrack'
    return provider
  }

  function openTrackerPrefs(): void {
    showPreferences('issues')
  }

  function browseIssues(connectionId: string): void {
    showIssuePicker(connectionId)
  }
</script>

<CollapsibleSection title="ISSUES" sectionKey="issues" borderTop>
  {#if loading}
    <div class="loading">Loading...</div>
  {:else if connections.length === 0}
    <div class="empty-state">
      <button class="connect-btn" onclick={openTrackerPrefs}>
        <Plus size={14} />
        Connect Tracker
      </button>
    </div>
  {:else}
    <ul class="tracker-list">
      {#each connections as conn (conn.id)}
        <li>
          <button
            class="tracker-item"
            onclick={() => browseIssues(conn.id)}
            title="Browse issues from {conn.name}"
          >
            <SquareKanban size={14} />
            <span class="tracker-name">{conn.name}</span>
            <span class="tracker-provider">{providerLabel(conn.provider)}</span>
            <ExternalLink size={12} />
          </button>
        </li>
      {/each}
    </ul>
    <div class="add-row">
      <button class="add-btn" onclick={openTrackerPrefs} title="Manage connections">
        <Plus size={12} />
        <span>Add</span>
      </button>
    </div>
  {/if}
</CollapsibleSection>

<style>
  .loading {
    padding: 8px 12px;
    font-size: 11px;
    color: var(--c-text-muted);
  }

  .empty-state {
    padding: 8px 12px;
  }

  .connect-btn {
    display: flex;
    align-items: center;
    gap: 6px;
    width: 100%;
    padding: 6px 10px;
    border: 1px dashed var(--c-border);
    border-radius: 6px;
    background: none;
    color: var(--c-text-muted);
    font-size: 12px;
    font-family: inherit;
    cursor: pointer;
    transition:
      border-color 0.1s,
      color 0.1s;
  }

  .connect-btn:hover {
    border-color: var(--c-accent-muted);
    color: var(--c-accent-text);
  }

  .tracker-list {
    list-style: none;
    padding: 0;
    margin: 0;
  }

  .tracker-item {
    display: flex;
    align-items: center;
    gap: 8px;
    width: 100%;
    height: 28px;
    padding: 0 12px;
    border: none;
    background: none;
    color: var(--c-text);
    font-size: 12px;
    font-family: inherit;
    cursor: pointer;
    text-align: left;
    transition: background 0.1s;
  }

  .tracker-item:hover {
    background: var(--c-hover);
  }

  .tracker-name {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    flex: 1;
  }

  .tracker-provider {
    font-size: 10px;
    color: var(--c-text-faint);
    flex-shrink: 0;
  }

  .add-row {
    padding: 4px 12px 4px;
  }

  .add-btn {
    display: flex;
    align-items: center;
    gap: 4px;
    padding: 2px 6px;
    border: none;
    background: none;
    color: var(--c-text-faint);
    font-size: 11px;
    font-family: inherit;
    cursor: pointer;
    transition: color 0.1s;
  }

  .add-btn:hover {
    color: var(--c-text-secondary);
  }
</style>
