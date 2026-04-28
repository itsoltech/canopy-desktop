<script lang="ts">
  import { onDestroy } from 'svelte'
  import { prefs, setPref } from '../../lib/stores/preferences.svelte'
  import CustomSelect from '../shared/CustomSelect.svelte'
  import CustomCheckbox from '../shared/CustomCheckbox.svelte'
  import PrefsSection from './_partials/PrefsSection.svelte'
  import PrefsRow from './_partials/PrefsRow.svelte'

  let autoUpdate = $derived(prefs['update.autoUpdate'] !== 'false')
  let channel = $derived(prefs['update.channel'] || 'stable')
  let checkFrequency = $derived(prefs['update.checkFrequency'] || 'daily')

  let checkState: 'idle' | 'checking' | 'up-to-date' | 'error' = $state('idle')
  let checkCleanup: (() => void) | null = $state(null)
  let dismissTimer: ReturnType<typeof setTimeout> | null = null

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
      if (dismissTimer) clearTimeout(dismissTimer)
      dismissTimer = setTimeout(() => {
        dismissTimer = null
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

  onDestroy(() => {
    checkCleanup?.()
    if (dismissTimer) clearTimeout(dismissTimer)
  })
</script>

<div class="flex flex-col gap-7">
  <PrefsSection title="Automatic updates">
    <PrefsRow
      label="Download and install automatically"
      help="Updates are fetched in the background and applied on the next restart"
      search="auto download install background"
    >
      <CustomCheckbox checked={autoUpdate} onchange={toggleAutoUpdate} />
    </PrefsRow>

    <PrefsRow
      label="Update channel"
      help="Stable receives tested releases. Pre-release includes the newest features but may have bugs."
      search="channel stable next pre-release beta"
    >
      <CustomSelect
        value={channel}
        options={[
          { value: 'stable', label: 'Stable' },
          { value: 'next', label: 'Pre-release' },
        ]}
        onchange={setChannel}
        maxWidth="180px"
      />
    </PrefsRow>

    <PrefsRow
      label="Check frequency"
      help="How often the app checks for new versions in the background"
      search="check frequency hourly daily weekly never manual"
    >
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
    </PrefsRow>
  </PrefsSection>

  <PrefsSection title="Manual check">
    <PrefsRow
      label="Check for updates"
      help="Run a one-off check against the current channel"
      search="check now manual update"
    >
      <div class="flex items-center gap-3">
        <button
          type="button"
          class="px-3 py-1 rounded-md text-sm font-inherit cursor-pointer border border-border bg-border-subtle text-text-secondary hover:bg-active hover:text-text disabled:opacity-50 disabled:cursor-default"
          onclick={checkNow}
          disabled={checkState === 'checking'}
        >
          {checkState === 'checking' ? 'Checking…' : 'Check now'}
        </button>
        <span class="text-sm" role="status" aria-live="polite">
          {#if checkState === 'up-to-date'}
            <span class="text-success">You're up to date</span>
          {:else if checkState === 'error'}
            <span class="text-danger">Check failed</span>
          {/if}
        </span>
      </div>
    </PrefsRow>
  </PrefsSection>
</div>
