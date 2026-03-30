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
    color: rgba(255, 255, 255, 0.4);
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
    border: 1px dashed rgba(255, 255, 255, 0.15);
    border-radius: 6px;
    background: none;
    color: rgba(255, 255, 255, 0.5);
    font-size: 12px;
    font-family: inherit;
    cursor: pointer;
    transition:
      border-color 0.1s,
      color 0.1s;
  }

  .connect-btn:hover {
    border-color: rgba(116, 192, 252, 0.4);
    color: rgba(116, 192, 252, 0.9);
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
    color: rgba(255, 255, 255, 0.7);
    font-size: 12px;
    font-family: inherit;
    cursor: pointer;
    text-align: left;
    transition: background 0.1s;
  }

  .tracker-item:hover {
    background: rgba(255, 255, 255, 0.06);
  }

  .tracker-name {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    flex: 1;
  }

  .tracker-provider {
    font-size: 10px;
    color: rgba(255, 255, 255, 0.35);
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
    color: rgba(255, 255, 255, 0.35);
    font-size: 11px;
    font-family: inherit;
    cursor: pointer;
    transition: color 0.1s;
  }

  .add-btn:hover {
    color: rgba(255, 255, 255, 0.7);
  }
</style>
