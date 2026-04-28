<script lang="ts">
  import { prefs, setPref } from '../../../lib/stores/preferences.svelte'
  import CustomSelect from '../../shared/CustomSelect.svelte'

  let model = $derived(prefs['claude.model'] || '')
  let permissionMode = $derived(prefs['claude.permissionMode'] || '')

  function updateModel(e: Event): void {
    setPref('claude.model', (e.target as HTMLInputElement).value)
  }
</script>

<div class="flex flex-col items-center text-center gap-4">
  <h2 class="m-0 text-lg font-semibold text-text">AI assistant</h2>
  <p class="m-0 text-md text-text-secondary max-w-[380px] leading-normal">
    Configure Claude Code defaults. You can change these anytime in Preferences.
  </p>

  <div class="flex flex-col gap-3.5 w-full max-w-[360px] text-left">
    <div class="flex flex-col gap-1">
      <label
        class="text-sm font-medium text-text-secondary uppercase tracking-[0.5px]"
        for="onboard-model">Model</label
      >
      <input
        id="onboard-model"
        class="px-2.5 py-2 border border-border rounded-lg bg-hover text-text text-md font-mono outline-none focus:border-focus-ring placeholder:text-text-faint"
        type="text"
        placeholder="sonnet, opus, haiku, or model ID"
        value={model}
        onchange={updateModel}
        spellcheck="false"
      />
      <span class="text-xs text-text-faint">Leave empty to use the default model.</span>
    </div>

    <div class="flex flex-col gap-1">
      <label
        class="text-sm font-medium text-text-secondary uppercase tracking-[0.5px]"
        for="onboard-perm">Permission mode</label
      >
      <CustomSelect
        id="onboard-perm"
        value={permissionMode}
        options={[
          { value: '', label: 'Default' },
          { value: 'plan', label: 'Plan' },
          { value: 'auto', label: 'Auto' },
          { value: 'acceptEdits', label: 'Accept edits' },
          { value: 'bypassPermissions', label: 'Bypass permissions' },
        ]}
        onchange={(v) => setPref('claude.permissionMode', v)}
      />
    </div>
  </div>
</div>
