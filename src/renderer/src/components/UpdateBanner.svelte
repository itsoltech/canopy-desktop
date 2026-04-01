<script lang="ts">
  import { updateState, installUpdate, dismissUpdate } from '../lib/stores/updateState.svelte'

  let state = $derived(updateState.status)
  let version = $derived(updateState.version)
  let percent = $derived(updateState.percent)
  let errorMessage = $derived(updateState.errorMessage)

  let visible = $derived(state !== 'idle' && !updateState.dismissed)
</script>

{#if visible}
  <div class="update-banner" class:error={state === 'error'} role="alert" aria-live="polite">
    {#if state === 'up-to-date'}
      <span class="text">You're up to date!</span>
    {:else if state === 'available'}
      <span class="text">Update v{version} available</span>
      <button class="btn" onclick={dismissUpdate}>Dismiss</button>
    {:else if state === 'downloading'}
      <span class="text">Downloading update… {percent}%</span>
      <div class="progress-track">
        <div class="progress-fill" style="width: {percent}%"></div>
      </div>
    {:else if state === 'installing'}
      <span class="text">Installing update…</span>
    {:else if state === 'ready'}
      <span class="text">Update v{version} ready</span>
      <button class="btn primary" onclick={installUpdate}>Restart Now</button>
      <button class="btn" onclick={dismissUpdate}>Later</button>
    {:else if state === 'error'}
      <span class="text">Update failed: {errorMessage}</span>
    {/if}
  </div>
{/if}

<style>
  .update-banner {
    position: fixed;
    bottom: 16px;
    right: 16px;
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 10px 16px;
    background: var(--c-bg-overlay);
    border: 1px solid var(--c-border);
    border-radius: 8px;
    backdrop-filter: blur(12px);
    -webkit-backdrop-filter: blur(12px);
    z-index: 9999;
    font-size: 13px;
    color: var(--c-text);
    max-width: 400px;
    box-shadow: 0 4px 20px rgba(0, 0, 0, 0.4);
  }

  .update-banner.error {
    border-color: var(--c-danger-bg);
  }

  .text {
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .progress-track {
    flex: 1;
    min-width: 80px;
    height: 4px;
    background: var(--c-hover-strong);
    border-radius: 2px;
    overflow: hidden;
  }

  .progress-fill {
    height: 100%;
    background: var(--c-accent-text);
    border-radius: 2px;
    transition: width 0.3s ease;
  }

  .btn {
    padding: 4px 10px;
    border: 1px solid var(--c-border);
    border-radius: 4px;
    background: transparent;
    color: var(--c-text);
    font-size: 12px;
    cursor: pointer;
    white-space: nowrap;
  }

  .btn:hover {
    background: var(--c-active);
  }

  .btn.primary {
    background: var(--c-accent-bg);
    border-color: var(--c-accent-muted);
    color: var(--c-accent-text);
  }

  .btn.primary:hover {
    background: var(--c-accent-bg-hover);
  }
</style>
