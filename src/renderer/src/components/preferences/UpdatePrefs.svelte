<script lang="ts">
  import { onDestroy } from 'svelte'
  import { prefs, setPref } from '../../lib/stores/preferences.svelte'
  import CustomSelect from '../shared/CustomSelect.svelte'
  import CustomCheckbox from '../shared/CustomCheckbox.svelte'

  let autoUpdate = $derived(prefs['update.autoUpdate'] !== 'false')
  let channel = $derived(prefs['update.channel'] || 'stable')
  let checkFrequency = $derived(prefs['update.checkFrequency'] || 'daily')

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

  function setCheckFrequency(value: string): void {
    setPref('update.checkFrequency', value)
    window.api.setUpdateCheckFrequency(value)
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

<div class="flex flex-col gap-4">
  <h3 class="text-[15px] font-semibold text-text m-0">Updates</h3>

  <label class="flex items-center gap-2 text-md text-text cursor-pointer">
    <CustomCheckbox checked={autoUpdate} onchange={toggleAutoUpdate} />
    <span>Automatically download and install updates</span>
  </label>

  <div class="flex items-center gap-3 text-md">
    <span class="text-text min-w-40">Update channel</span>
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
  <div class="text-xs text-text-muted leading-normal -mt-2">
    Stable gets tested releases. Pre-release includes newest features but may have bugs
  </div>

  <div class="flex items-center gap-3 text-md">
    <span class="text-text min-w-40">Check frequency</span>
    <CustomSelect
      value={checkFrequency}
      options={[
        { value: 'hourly', label: 'Every hour' },
        { value: 'daily', label: 'Every day' },
        { value: 'weekly', label: 'Every week' },
        { value: 'never', label: 'Never (manual only)' },
      ]}
      onchange={setCheckFrequency}
      maxWidth="180px"
    />
  </div>
  <div class="text-xs text-text-muted leading-normal -mt-2">
    How often the app checks for new versions in the background
  </div>

  <div class="flex items-center gap-3">
    <button
      class="px-3.5 py-1.5 border border-text-faint rounded-lg bg-hover text-text text-md font-inherit cursor-pointer transition-colors duration-fast enabled:hover:bg-hover-strong disabled:opacity-50 disabled:cursor-default"
      onclick={checkNow}
      disabled={checkState === 'checking'}
    >
      {checkState === 'checking' ? 'Checking...' : 'Check for updates'}
    </button>
    {#if checkState === 'up-to-date'}
      <span class="text-sm text-success">You're up to date</span>
    {:else if checkState === 'error'}
      <span class="text-sm text-danger">Check failed</span>
    {/if}
  </div>
</div>
