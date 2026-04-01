<script lang="ts">
  import { prefs, setPref } from '../../../lib/stores/preferences.svelte'
  import CustomSelect from '../../shared/CustomSelect.svelte'

  let model = $derived(prefs['claude.model'] || '')
  let permissionMode = $derived(prefs['claude.permissionMode'] || '')

  function updateModel(e: Event): void {
    setPref('claude.model', (e.target as HTMLInputElement).value)
  }
</script>

<div class="step">
  <h2 class="title">AI assistant</h2>
  <p class="description">
    Configure Claude Code defaults. You can change these anytime in Preferences.
  </p>

  <div class="form">
    <div class="field">
      <label class="field-label" for="onboard-model">Model</label>
      <input
        id="onboard-model"
        class="text-input"
        type="text"
        placeholder="sonnet, opus, haiku, or model ID"
        value={model}
        onchange={updateModel}
        spellcheck="false"
      />
      <span class="field-hint">Leave empty to use the default model.</span>
    </div>

    <div class="field">
      <label class="field-label" for="onboard-perm">Permission mode</label>
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

<style>
  .step {
    display: flex;
    flex-direction: column;
    align-items: center;
    text-align: center;
    gap: 16px;
  }

  .title {
    margin: 0;
    font-size: 18px;
    font-weight: 600;
    color: var(--c-text);
  }

  .description {
    margin: 0;
    font-size: 13px;
    color: var(--c-text-secondary);
    max-width: 380px;
    line-height: 1.5;
  }

  .form {
    display: flex;
    flex-direction: column;
    gap: 14px;
    width: 100%;
    max-width: 360px;
    text-align: left;
  }

  .field {
    display: flex;
    flex-direction: column;
    gap: 4px;
  }

  .field-label {
    font-size: 12px;
    font-weight: 500;
    color: var(--c-text-secondary);
    text-transform: uppercase;
    letter-spacing: 0.5px;
  }

  .field-hint {
    font-size: 11px;
    color: var(--c-text-faint);
  }

  .text-input {
    padding: 8px 10px;
    border: 1px solid var(--c-border);
    border-radius: 6px;
    background: var(--c-hover);
    color: var(--c-text);
    font-size: 13px;
    font-family: monospace;
    outline: none;
  }

  .text-input:focus {
    border-color: var(--c-focus-ring);
  }

  .text-input::placeholder {
    color: var(--c-text-faint);
  }
</style>
