<script lang="ts">
  import { onMount } from 'svelte'
  import { SquareKanban, Plus, ExternalLink, Settings } from '@lucide/svelte'
  import CollapsibleSection from './CollapsibleSection.svelte'
  import {
    getRepoConfig,
    getHasCredentials,
    isTaskTrackerLoading,
    getTaskTrackerConnections,
    loadConnections,
  } from '../../lib/stores/taskTracker.svelte'
  import { showPreferences } from '../../lib/stores/dialogs.svelte'
  import { showTaskPicker } from '../../lib/stores/dialogs.svelte'

  onMount(() => {
    loadConnections()
  })

  let config = $derived(getRepoConfig())
  let hasCreds = $derived(getHasCredentials())
  let loading = $derived(isTaskTrackerLoading())
  let connections = $derived(getTaskTrackerConnections())

  function providerLabel(provider: string): string {
    if (provider === 'jira') return 'Jira'
    if (provider === 'youtrack') return 'YouTrack'
    return provider
  }

  function openTrackerPrefs(): void {
    showPreferences('tasks')
  }

  function browseTasks(connectionId: string): void {
    showTaskPicker(connectionId)
  }
</script>

<CollapsibleSection title="TASKS" sectionKey="tasks" borderTop>
  {#if loading}
    <div class="loading">Loading...</div>
  {:else if config}
    <ul class="tracker-list">
      <li>
        <button
          class="tracker-item"
          onclick={() => {
            if (connections.length > 0) {
              browseTasks(connections[0].id)
            }
          }}
          disabled={!hasCreds || connections.length === 0}
          title={hasCreds
            ? `Browse tasks — ${providerLabel(config.tracker.provider)}`
            : 'Credentials required'}
        >
          <SquareKanban size={14} />
          <span class="tracker-name">{config.tracker.baseUrl || 'Not configured'}</span>
          <span class="tracker-provider">{providerLabel(config.tracker.provider)}</span>
          {#if hasCreds}
            <ExternalLink size={12} />
          {/if}
        </button>
      </li>
    </ul>
    {#if !hasCreds}
      <div class="token-hint">
        <button class="connect-btn" onclick={openTrackerPrefs}>
          Credentials required — configure in Preferences
        </button>
      </div>
    {/if}
    <div class="add-row">
      <button class="add-btn" onclick={openTrackerPrefs} title="Configure tracker">
        <Settings size={12} />
        <span>Settings</span>
      </button>
    </div>
  {:else}
    <div class="empty-state">
      <button class="connect-btn" onclick={openTrackerPrefs}>
        <Plus size={14} />
        Configure Tracker
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

  .token-hint {
    padding: 4px 12px;
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

  .tracker-item:hover:not(:disabled) {
    background: var(--c-hover);
  }

  .tracker-item:disabled {
    opacity: 0.5;
    cursor: default;
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
