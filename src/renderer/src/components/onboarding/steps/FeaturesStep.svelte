<script lang="ts">
  import { prefs, setPref } from '../../../lib/stores/preferences.svelte'
  import CustomCheckbox from '../../shared/CustomCheckbox.svelte'

  let reopenLast = $derived(prefs.reopenLastWorkspace !== 'false')
  let notchEnabled = $derived(prefs['notch.enabled'] === 'true')
  let wpmEnabled = $derived(prefs['wpm.enabled'] === 'true')
  let telemetryEnabled = $derived(prefs['telemetry.enabled'] !== 'false')

  const isMac = navigator.userAgent.includes('Mac')

  function toggle(key: string, current: boolean): void {
    setPref(key, current ? 'false' : 'true')
  }

  function toggleNotch(): void {
    const next = !notchEnabled
    setPref('notch.enabled', next ? 'true' : 'false')
    window.api.setNotchEnabled(next)
  }
</script>

<div class="flex flex-col items-center text-center gap-4">
  <h2 class="m-0 text-lg font-semibold text-text">Customize features</h2>
  <p class="m-0 text-md text-text-secondary">
    Toggle the features you want. All of these can be changed later.
  </p>

  <div class="flex flex-col gap-0.5 w-full max-w-[400px] text-left">
    <label
      class="flex items-start gap-2.5 px-3 py-2.5 rounded-xl transition-colors duration-fast hover:bg-border-subtle"
    >
      <CustomCheckbox
        checked={reopenLast}
        onchange={() => toggle('reopenLastWorkspace', reopenLast)}
      />
      <div class="flex flex-col gap-0.5">
        <span class="text-md text-text">Reopen last workspace on startup</span>
        <span class="text-xs text-text-muted leading-snug"
          >Resume where you left off when launching Canopy.</span
        >
      </div>
    </label>

    {#if isMac}
      <label
        class="flex items-start gap-2.5 px-3 py-2.5 rounded-xl transition-colors duration-fast hover:bg-border-subtle"
      >
        <CustomCheckbox checked={notchEnabled} onchange={toggleNotch} />
        <div class="flex flex-col gap-0.5">
          <span class="text-md text-text">Notch overlay</span>
          <span class="text-xs text-text-muted leading-snug"
            >Show AI session status in the MacBook notch area.</span
          >
        </div>
      </label>
    {/if}

    <label
      class="flex items-start gap-2.5 px-3 py-2.5 rounded-xl transition-colors duration-fast hover:bg-border-subtle"
    >
      <CustomCheckbox checked={wpmEnabled} onchange={() => toggle('wpm.enabled', wpmEnabled)} />
      <div class="flex flex-col gap-0.5">
        <span class="text-md text-text">Typing speed (WPM)</span>
        <span class="text-xs text-text-muted leading-snug"
          >Display words-per-minute in terminals.</span
        >
      </div>
    </label>

    <label
      class="flex items-start gap-2.5 px-3 py-2.5 rounded-xl transition-colors duration-fast hover:bg-border-subtle"
    >
      <CustomCheckbox
        checked={telemetryEnabled}
        onchange={() => toggle('telemetry.enabled', telemetryEnabled)}
      />
      <div class="flex flex-col gap-0.5">
        <span class="text-md text-text">Minimal telemetry</span>
        <span class="text-xs text-text-muted leading-snug"
          >Send one daily ping to count active users. No personal data is collected.</span
        >
      </div>
    </label>
  </div>
</div>
