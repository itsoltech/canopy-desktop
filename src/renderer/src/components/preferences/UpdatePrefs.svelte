<script lang="ts">
  import { onDestroy } from 'svelte'
  import { prefs, setPref } from '../../lib/stores/preferences.svelte'
  import CustomSelect from '../shared/CustomSelect.svelte'
  import CustomCheckbox from '../shared/CustomCheckbox.svelte'

  let autoUpdate = $derived(prefs['update.autoUpdate'] !== 'false')
  let channel = $derived(prefs['update.channel'] || 'stable')

  let checkState: 'idle' | 'checking' | 'up-to-date' | 'error' = $state('idle')
  let checkCleanup: (() => void) | null = $state(null)

  function toggleAutoUpdate(): void {
    const next = !autoUpdate
    setPref('update.autoUpdate', next ? 'true' : 'false')
    window.api.setAutoUpdate(next)
  }

  function setChannel(value: string): void {
    setPref('update.channel', value)
    window.api.setUpdateChannel(value)
  }

  function checkNow(): void {
    checkCleanup?.()
    checkState = 'checking'

    const dismiss = (state: 'up-to-date' | 'error'): void => {
      checkState = state
      setTimeout(() => {
        if (checkState === state) checkState = 'idle'
      }, 4000)
    }

    const offAvailable = window.api.onUpdateAvailable(() => {
      cleanup()
    })
    const offNotAvailable = window.api.onUpdateNotAvailable(() => {
      dismiss('up-to-date')
      cleanup()
    })
    const offError = window.api.onUpdateError(() => {
      dismiss('error')
      cleanup()
    })

    const cleanup = (): void => {
      offAvailable()
      offNotAvailable()
      offError()
      checkCleanup = null
    }

    checkCleanup = cleanup
    window.api.checkForUpdates()
  }

  onDestroy(() => checkCleanup?.())
</script>

<div class="section">
  <h3 class="section-title">Updates</h3>

  <label class="checkbox-row" onclick={toggleAutoUpdate}>
    <CustomCheckbox checked={autoUpdate} onchange={toggleAutoUpdate} />
    <span>Automatically download and install updates</span>
  </label>

  <div class="select-row">
    <span class="select-label">Update channel</span>
    <CustomSelect
      value={channel}
      options={[
        { value: 'stable', label: 'Stable' },
        { value: 'next', label: 'Pre-release' },
      ]}
      onchange={setChannel}
      maxWidth="180px"
    />
  </div>

  <div class="check-row">
    <button class="check-btn" onclick={checkNow} disabled={checkState === 'checking'}>
      {checkState === 'checking' ? 'Checking...' : 'Check for updates'}
    </button>
    {#if checkState === 'up-to-date'}
      <span class="check-status ok">You're up to date</span>
    {:else if checkState === 'error'}
      <span class="check-status err">Check failed</span>
    {/if}
  </div>
</div>

<style>
  .section {
    display: flex;
    flex-direction: column;
    gap: 16px;
  }

  .section-title {
    font-size: 15px;
    font-weight: 600;
    color: var(--c-text);
    margin: 0;
  }

  .checkbox-row {
    display: flex;
    align-items: center;
    gap: 8px;
    font-size: 13px;
    color: var(--c-text);
    cursor: pointer;
  }

  .select-row {
    display: flex;
    align-items: center;
    gap: 12px;
    font-size: 13px;
  }

  .select-label {
    color: var(--c-text);
    min-width: 160px;
  }

  .check-row {
    display: flex;
    align-items: center;
    gap: 12px;
  }

  .check-btn {
    padding: 6px 14px;
    border: 1px solid var(--c-text-faint);
    border-radius: 6px;
    background: var(--c-hover);
    color: var(--c-text);
    font-size: 13px;
    font-family: inherit;
    cursor: pointer;
    transition:
      background 0.15s,
      border-color 0.15s;
  }

  .check-btn:hover:not(:disabled) {
    background: var(--c-hover-strong);
    border-color: var(--c-text-faint);
  }

  .check-btn:disabled {
    opacity: 0.5;
    cursor: default;
  }

  .check-status {
    font-size: 12px;
  }

  .check-status.ok {
    color: var(--c-success);
  }

  .check-status.err {
    color: var(--c-danger);
  }
</style>
