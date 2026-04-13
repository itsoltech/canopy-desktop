<script lang="ts">
  import type { ProfilePrefs } from '../../../../main/profiles/types'
  import CustomSelect from '../shared/CustomSelect.svelte'
  import ProfileEnvVarsSection from './ProfileEnvVarsSection.svelte'

  let {
    prefs,
    apiKey,
    hasApiKey,
    onPrefsChange,
    onApiKeyChange,
  }: {
    prefs: ProfilePrefs
    apiKey: string
    hasApiKey: boolean
    onPrefsChange: (next: ProfilePrefs) => void
    onApiKeyChange: (next: string) => void
  } = $props()

  function set<K extends keyof ProfilePrefs>(key: K, value: ProfilePrefs[K]): void {
    onPrefsChange({ ...prefs, [key]: value })
  }

  function onTextInput<K extends keyof ProfilePrefs>(key: K) {
    return (e: Event): void => {
      const target = e.target as HTMLInputElement | HTMLTextAreaElement
      set(key, target.value as ProfilePrefs[K])
    }
  }
</script>

<div class="subsection">
  <h4 class="subsection-title">Model & behavior</h4>

  <div class="field">
    <label class="field-label" for="gemini-model">Model</label>
    <input
      id="gemini-model"
      class="text-input"
      type="text"
      value={prefs.model ?? ''}
      oninput={onTextInput('model')}
      placeholder="Default"
      spellcheck="false"
    />
    <span class="field-hint">Leave empty for Gemini CLI default</span>
  </div>

  <div class="field">
    <label class="field-label" for="gemini-approval">Approval mode</label>
    <span class="field-hint"
      >Controls what Gemini can do without asking. YOLO = full autonomy, Plan = read-only</span
    >
    <CustomSelect
      id="gemini-approval"
      value={prefs.approvalMode ?? ''}
      options={[
        { value: '', label: 'Default' },
        { value: 'default', label: 'Prompt' },
        { value: 'auto_edit', label: 'Auto Edit' },
        { value: 'yolo', label: 'YOLO' },
        { value: 'plan', label: 'Plan (read-only)' },
      ]}
      onchange={(v) => set('approvalMode', v)}
    />
  </div>
</div>

<div class="subsection">
  <h4 class="subsection-title">API</h4>

  <div class="field">
    <label class="field-label" for="gemini-apikey">API key</label>
    <span class="field-hint">Google AI API key. Falls back to GEMINI_API_KEY env variable</span>
    <input
      id="gemini-apikey"
      class="text-input"
      type="password"
      value={apiKey}
      oninput={(e) => onApiKeyChange((e.target as HTMLInputElement).value)}
      placeholder={hasApiKey ? '•••• (saved — leave empty to keep)' : 'Uses GEMINI_API_KEY'}
      spellcheck="false"
      autocomplete="off"
    />
  </div>
</div>

<ProfileEnvVarsSection
  customEnv={prefs.customEnv}
  hint="Extra env vars passed to Gemini CLI sessions in this profile"
  onChange={(v) => set('customEnv', v)}
/>

<div class="subsection">
  <h4 class="subsection-title">Advanced</h4>

  <div class="field">
    <label class="field-label" for="gemini-settings">Settings JSON override</label>
    <textarea
      id="gemini-settings"
      class="text-input textarea mono"
      rows="4"
      value={prefs.settingsJson ?? ''}
      oninput={onTextInput('settingsJson')}
      placeholder={'{"key": "value"}'}
      spellcheck="false"
    ></textarea>
    <span class="field-hint"
      >Merged into per-session .gemini/settings.json (hooks always preserved)</span
    >
  </div>
</div>

<style>
  .subsection {
    display: flex;
    flex-direction: column;
    gap: 10px;
    margin-bottom: 20px;
  }
  .subsection-title {
    font-size: 11px;
    font-weight: 600;
    text-transform: uppercase;
    letter-spacing: 0.5px;
    color: var(--c-text-muted);
    margin: 0;
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
  }
  .field-hint {
    font-size: 11px;
    color: var(--c-text-faint);
  }
  .text-input {
    padding: 6px 10px;
    border: 1px solid var(--c-border);
    border-radius: 6px;
    background: var(--c-hover);
    color: var(--c-text);
    font-size: 13px;
    font-family: inherit;
    outline: none;
  }
  .text-input:focus {
    border-color: var(--c-focus-ring);
  }
  .textarea {
    resize: vertical;
    min-height: 60px;
  }
  .mono {
    font-family: monospace;
    font-size: 12px;
  }
</style>
