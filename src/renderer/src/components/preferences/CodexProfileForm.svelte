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

  let fullAuto = $derived(prefs.fullAuto === 'true')
</script>

<div class="subsection">
  <h4 class="subsection-title">Model & behavior</h4>

  <div class="field">
    <label class="field-label" for="codex-model">Model</label>
    <input
      id="codex-model"
      class="text-input"
      type="text"
      value={prefs.model ?? ''}
      oninput={onTextInput('model')}
      placeholder="Default"
      spellcheck="false"
    />
    <span class="field-hint">Leave empty for Codex default</span>
  </div>

  <div class="field">
    <label class="field-label" for="codex-approval">Approval mode</label>
    <span class="field-hint">Controls when Codex pauses for human approval</span>
    <CustomSelect
      id="codex-approval"
      value={prefs.approvalMode ?? ''}
      options={[
        { value: '', label: 'Default' },
        { value: 'untrusted', label: 'Untrusted' },
        { value: 'on-request', label: 'On Request' },
        { value: 'never', label: 'Never' },
      ]}
      onchange={(v) => set('approvalMode', v)}
    />
  </div>

  <div class="field">
    <label class="field-label" for="codex-sandbox">Sandbox</label>
    <span class="field-hint">Command execution policy</span>
    <CustomSelect
      id="codex-sandbox"
      value={prefs.sandbox ?? ''}
      options={[
        { value: '', label: 'Default' },
        { value: 'read-only', label: 'Read Only' },
        { value: 'workspace-write', label: 'Workspace Write' },
        { value: 'danger-full-access', label: 'Full Access' },
      ]}
      onchange={(v) => set('sandbox', v)}
    />
  </div>

  <div class="field">
    <label class="field-label checkbox-label">
      <input
        type="checkbox"
        checked={fullAuto}
        onchange={() => set('fullAuto', fullAuto ? '' : 'true')}
      />
      Full auto
    </label>
    <span class="field-hint">Workspace-write sandbox + on-request approvals</span>
  </div>

  <div class="field">
    <label class="field-label" for="codex-profile">Profile</label>
    <input
      id="codex-profile"
      class="text-input"
      type="text"
      value={prefs.profile ?? ''}
      oninput={onTextInput('profile')}
      placeholder="Default"
      spellcheck="false"
    />
    <span class="field-hint">Configuration profile from config.toml</span>
  </div>
</div>

<div class="subsection">
  <h4 class="subsection-title">API</h4>

  <div class="field">
    <label class="field-label" for="codex-apikey">API key</label>
    <span class="field-hint">OpenAI API key. Falls back to OPENAI_API_KEY env variable</span>
    <input
      id="codex-apikey"
      class="text-input"
      type="password"
      value={apiKey}
      oninput={(e) => onApiKeyChange((e.target as HTMLInputElement).value)}
      placeholder={hasApiKey ? '•••• (saved — leave empty to keep)' : 'Uses OPENAI_API_KEY'}
      spellcheck="false"
      autocomplete="off"
    />
  </div>

  <div class="field">
    <label class="field-label" for="codex-baseurl">Base URL</label>
    <input
      id="codex-baseurl"
      class="text-input"
      type="text"
      value={prefs.baseUrl ?? ''}
      oninput={onTextInput('baseUrl')}
      placeholder="https://api.openai.com"
      spellcheck="false"
    />
    <span class="field-hint">Custom API endpoint (Ollama, vLLM, OpenAI-compatible proxy)</span>
  </div>
</div>

<ProfileEnvVarsSection
  customEnv={prefs.customEnv}
  hint="Extra env vars passed to Codex sessions in this profile"
  onChange={(v) => set('customEnv', v)}
/>

<div class="subsection">
  <h4 class="subsection-title">Advanced</h4>

  <div class="field">
    <label class="field-label" for="codex-settings">Settings JSON override</label>
    <textarea
      id="codex-settings"
      class="text-input textarea mono"
      rows="4"
      value={prefs.settingsJson ?? ''}
      oninput={onTextInput('settingsJson')}
      placeholder={'{"key": "value"}'}
      spellcheck="false"
    ></textarea>
    <span class="field-hint">Merged into per-session .codex/hooks.json</span>
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
  .checkbox-label {
    display: flex;
    align-items: center;
    gap: 8px;
    font-weight: 500;
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
