<script lang="ts">
  import { workspaceState, selectWorktree } from '../../lib/stores/workspace.svelte'
  import { showCreateWorktree } from '../../lib/stores/dialogs.svelte'
</script>

<section class="sidebar-section">
  <div class="section-header">
    <h3 class="section-title">WORKTREES</h3>
    <button class="new-btn" onclick={showCreateWorktree} title="Create worktree">+ new</button>
  </div>
  <ul class="worktree-list">
    {#each workspaceState.worktrees as wt (wt.path)}
      <li>
        <button
          class="worktree-item"
          class:active={wt.path === workspaceState.selectedWorktreePath}
          onclick={() => selectWorktree(wt.path)}
        >
          <span class="indicator">{wt.isMain ? '*' : ' '}</span>
          <span class="branch-name">{wt.branch}</span>
        </button>
      </li>
    {/each}
  </ul>
</section>

<style>
  .sidebar-section {
    padding: 12px 0;
  }

  .section-header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 0 12px 8px;
  }

  .section-title {
    font-size: 10px;
    font-weight: 600;
    letter-spacing: 1px;
    color: rgba(255, 255, 255, 0.4);
    text-transform: uppercase;
  }

  .new-btn {
    font-size: 10px;
    font-weight: 500;
    font-family: inherit;
    color: rgba(255, 255, 255, 0.35);
    background: none;
    border: none;
    padding: 1px 5px;
    border-radius: 4px;
    cursor: pointer;
    transition:
      color 0.1s,
      background 0.1s;
  }

  .new-btn:hover {
    color: rgba(255, 255, 255, 0.7);
    background: rgba(255, 255, 255, 0.08);
  }

  .worktree-list {
    list-style: none;
    padding: 0;
    margin: 0;
  }

  .worktree-item {
    display: flex;
    align-items: center;
    gap: 4px;
    width: 100%;
    padding: 4px 12px;
    border: none;
    background: none;
    color: rgba(255, 255, 255, 0.7);
    font-size: 12px;
    font-family: inherit;
    cursor: pointer;
    text-align: left;
  }

  .worktree-item:hover {
    background: rgba(255, 255, 255, 0.06);
    color: rgba(255, 255, 255, 0.9);
  }

  .worktree-item.active {
    background: rgba(255, 255, 255, 0.1);
    color: #fff;
  }

  .indicator {
    font-family: monospace;
    font-size: 11px;
    color: rgba(255, 255, 255, 0.5);
    width: 10px;
    flex-shrink: 0;
  }

  .branch-name {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
</style>
