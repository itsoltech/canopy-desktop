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
    <label class="field-label" for="codex-sdk-model">Model</label>
    <input
      id="codex-sdk-model"
      class="text-input"
      type="text"
      value={prefs.model ?? ''}
      oninput={onTextInput('model')}
      placeholder="Codex default"
      spellcheck="false"
    />
    <span class="field-hint">Leave empty to let Codex choose its default model</span>
  </div>

  <div class="field">
    <label class="field-label" for="codex-sdk-approval">Approval mode</label>
    <span class="field-hint">Controls when Codex asks before running commands or edits</span>
    <CustomSelect
      id="codex-sdk-approval"
      value={prefs.approvalMode ?? ''}
      options={[
        { value: '', label: 'Default' },
        { value: 'untrusted', label: 'Untrusted' },
        { value: 'on-request', label: 'On Request' },
        { value: 'on-failure', label: 'On Failure' },
        { value: 'never', label: 'Never' },
      ]}
      onchange={(v) => set('approvalMode', v)}
    />
  </div>

  <div class="field">
    <label class="field-label" for="codex-sdk-sandbox">Sandbox</label>
    <span class="field-hint">Command execution policy passed to Codex SDK</span>
    <CustomSelect
      id="codex-sdk-sandbox"
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
    <label class="field-label" for="codex-sdk-effort">Reasoning effort</label>
    <CustomSelect
      id="codex-sdk-effort"
      value={prefs.effortLevel ?? ''}
      options={[
        { value: '', label: 'Default' },
        { value: 'low', label: 'Low' },
        { value: 'medium', label: 'Medium' },
        { value: 'high', label: 'High' },
        { value: 'max', label: 'Max' },
      ]}
      onchange={(v) => set('effortLevel', v)}
    />
    <span class="field-hint">Max maps to Codex SDK xhigh</span>
  </div>
</div>

<div class="subsection">
  <h4 class="subsection-title">API</h4>

  <div class="field">
    <label class="field-label" for="codex-sdk-apikey">API key</label>
    <span class="field-hint">OpenAI API key. Falls back to Codex auth if left empty</span>
    <input
      id="codex-sdk-apikey"
      class="text-input"
      type="password"
      value={apiKey}
      oninput={(e) => onApiKeyChange((e.target as HTMLInputElement).value)}
      placeholder={hasApiKey ? '•••• (saved — leave empty to keep)' : 'Uses Codex/OpenAI auth'}
      spellcheck="false"
      autocomplete="off"
    />
  </div>

  <div class="field">
    <label class="field-label" for="codex-sdk-baseurl">Base URL</label>
    <input
      id="codex-sdk-baseurl"
      class="text-input"
      type="text"
      value={prefs.baseUrl ?? ''}
      oninput={onTextInput('baseUrl')}
      placeholder="https://api.openai.com"
      spellcheck="false"
    />
    <span class="field-hint">Optional OpenAI-compatible endpoint</span>
  </div>
</div>

<ProfileEnvVarsSection
  customEnv={prefs.customEnv}
  hint="Extra env vars passed to Codex SDK sessions in this profile"
  onChange={(v) => set('customEnv', v)}
/>

<div class="subsection">
  <h4 class="subsection-title">Advanced</h4>

  <div class="field">
    <label class="field-label" for="codex-sdk-settings">Config JSON override</label>
    <textarea
      id="codex-sdk-settings"
      class="text-input textarea mono"
      rows="4"
      value={prefs.settingsJson ?? ''}
      oninput={onTextInput('settingsJson')}
      placeholder={'{"show_raw_agent_reasoning": true}'}
      spellcheck="false"
    ></textarea>
    <span class="field-hint">Passed to Codex SDK as config overrides</span>
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
