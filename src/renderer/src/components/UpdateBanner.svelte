<script lang="ts">
  import { onMount } from 'svelte'

  type UpdateState =
    | 'idle'
    | 'up-to-date'
    | 'available'
    | 'downloading'
    | 'ready'
    | 'installing'
    | 'error'

  let state: UpdateState = $state('idle')
  let version = $state('')
  let percent = $state(0)
  let errorMessage = $state('')
  let dismissed = $state(false)

  onMount(() => {
    let autoDismissTimer: ReturnType<typeof setTimeout> | null = null

    const unsubs = [
      window.api.onUpdateAvailable((data) => {
        version = data.version
        state = 'available'
        dismissed = false
      }),
      window.api.onUpdateProgress((data) => {
        percent = Math.round(data.percent)
        state = 'downloading'
      }),
      window.api.onUpdateDownloaded((data) => {
        version = data.version
        state = 'ready'
        dismissed = false
      }),
      window.api.onUpdateNotAvailable(() => {
        state = 'up-to-date'
        dismissed = false
        if (autoDismissTimer) clearTimeout(autoDismissTimer)
        autoDismissTimer = setTimeout(() => {
          if (state === 'up-to-date') state = 'idle'
          autoDismissTimer = null
        }, 4000)
      }),
      window.api.onUpdateError((data) => {
        errorMessage = data.message
        state = 'error'
        dismissed = false
        if (autoDismissTimer) clearTimeout(autoDismissTimer)
        autoDismissTimer = setTimeout(() => {
          if (state === 'error') state = 'idle'
          autoDismissTimer = null
        }, 5000)
      }),
      window.api.onUpdateInstalling(() => {
        state = 'installing'
      }),
    ]
    return () => {
      unsubs.forEach((fn) => fn())
      if (autoDismissTimer) clearTimeout(autoDismissTimer)
    }
  })

  function restart(): void {
    window.api.installUpdate()
  }

  function dismiss(): void {
    dismissed = true
  }

  let visible = $derived(state !== 'idle' && !dismissed)
</script>

{#if visible}
  <div class="update-banner" class:error={state === 'error'} role="alert" aria-live="polite">
    {#if state === 'up-to-date'}
      <span class="text">You're up to date!</span>
    {:else if state === 'available'}
      <span class="text">Update v{version} available</span>
      <button class="btn" onclick={dismiss}>Dismiss</button>
    {:else if state === 'downloading'}
      <span class="text">Downloading update… {percent}%</span>
      <div class="progress-track">
        <div class="progress-fill" style="width: {percent}%"></div>
      </div>
    {:else if state === 'installing'}
      <span class="text">Installing update…</span>
    {:else if state === 'ready'}
      <span class="text">Update v{version} ready</span>
      <button class="btn primary" onclick={restart}>Restart Now</button>
      <button class="btn" onclick={dismiss}>Later</button>
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
    background: rgba(40, 40, 40, 0.95);
    border: 1px solid rgba(255, 255, 255, 0.1);
    border-radius: 8px;
    backdrop-filter: blur(12px);
    -webkit-backdrop-filter: blur(12px);
    z-index: 9999;
    font-size: 13px;
    color: rgba(255, 255, 255, 0.8);
    max-width: 400px;
    box-shadow: 0 4px 20px rgba(0, 0, 0, 0.4);
  }

  .update-banner.error {
    border-color: rgba(255, 80, 80, 0.3);
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
    background: rgba(255, 255, 255, 0.1);
    border-radius: 2px;
    overflow: hidden;
  }

  .progress-fill {
    height: 100%;
    background: rgba(100, 180, 255, 0.8);
    border-radius: 2px;
    transition: width 0.3s ease;
  }

  .btn {
    padding: 4px 10px;
    border: 1px solid rgba(255, 255, 255, 0.15);
    border-radius: 4px;
    background: transparent;
    color: rgba(255, 255, 255, 0.7);
    font-size: 12px;
    cursor: pointer;
    white-space: nowrap;
  }

  .btn:hover {
    background: rgba(255, 255, 255, 0.08);
  }

  .btn.primary {
    background: rgba(100, 180, 255, 0.2);
    border-color: rgba(100, 180, 255, 0.3);
    color: rgba(100, 180, 255, 0.9);
  }

  .btn.primary:hover {
    background: rgba(100, 180, 255, 0.3);
  }
</style>
