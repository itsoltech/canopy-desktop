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
    <label class="field-label" for="claude-model">Model</label>
    <span class="field-hint">Short name (sonnet, opus, haiku) or full model ID</span>
    <input
      id="claude-model"
      class="text-input"
      type="text"
      value={prefs.model ?? ''}
      oninput={onTextInput('model')}
      placeholder="sonnet, opus, haiku, or model ID"
      spellcheck="false"
    />
  </div>

  <div class="field">
    <label class="field-label" for="claude-perm">Permission mode</label>
    <span class="field-hint"
      >Controls what Claude can do without asking. Plan = read-only, Auto = full autonomy</span
    >
    <CustomSelect
      id="claude-perm"
      value={prefs.permissionMode ?? ''}
      options={[
        { value: '', label: 'Default' },
        { value: 'plan', label: 'Plan' },
        { value: 'auto', label: 'Auto' },
        { value: 'acceptEdits', label: 'Accept edits' },
        { value: 'bypassPermissions', label: 'Bypass permissions' },
      ]}
      onchange={(v) => set('permissionMode', v)}
    />
  </div>

  <div class="field">
    <label class="field-label" for="claude-effort">Effort level</label>
    <span class="field-hint">Higher effort means more thorough but slower responses</span>
    <CustomSelect
      id="claude-effort"
      value={prefs.effortLevel ?? ''}
      options={[
        { value: '', label: 'Default' },
        { value: 'low', label: 'Low' },
        { value: 'medium', label: 'Medium' },
        { value: 'high', label: 'High' },
        { value: 'xhigh', label: 'Extra High' },
        { value: 'max', label: 'Max' },
      ]}
      onchange={(v) => set('effortLevel', v)}
    />
  </div>
</div>

<div class="subsection">
  <h4 class="subsection-title">API / Provider</h4>

  <div class="field">
    <label class="field-label" for="claude-apikey">API key</label>
    <span class="field-hint">Anthropic API key. Falls back to ANTHROPIC_API_KEY env variable</span>
    <input
      id="claude-apikey"
      class="text-input"
      type="password"
      value={apiKey}
      oninput={(e) => onApiKeyChange((e.target as HTMLInputElement).value)}
      placeholder={hasApiKey ? '•••• (saved — leave empty to keep)' : 'sk-ant-...'}
      spellcheck="false"
      autocomplete="off"
    />
  </div>

  <div class="field">
    <label class="field-label" for="claude-baseurl">Base URL</label>
    <span class="field-hint"
      >Custom API endpoint. Use for Ollama, GLM, MinMax, or any OpenAI-compatible Anthropic proxy.</span
    >
    <input
      id="claude-baseurl"
      class="text-input"
      type="text"
      value={prefs.baseUrl ?? ''}
      oninput={onTextInput('baseUrl')}
      placeholder="https://api.anthropic.com"
      spellcheck="false"
    />
  </div>

  <div class="field">
    <label class="field-label" for="claude-provider">Provider</label>
    <span class="field-hint">Cloud provider for the Claude API backend</span>
    <CustomSelect
      id="claude-provider"
      value={prefs.provider ?? ''}
      options={[
        { value: '', label: 'Default (Anthropic)' },
        { value: 'bedrock', label: 'AWS Bedrock' },
        { value: 'vertex', label: 'Google Vertex AI' },
        { value: 'foundry', label: 'Microsoft Foundry' },
      ]}
      onchange={(v) => set('provider', v)}
    />
  </div>
</div>

<div class="subsection">
  <h4 class="subsection-title">System prompt</h4>

  <div class="field">
    <label class="field-label" for="claude-sysprompt">Append to system prompt</label>
    <span class="field-hint"
      >Extra instructions added after the default system prompt in every session</span
    >
    <textarea
      id="claude-sysprompt"
      class="text-input textarea"
      rows="3"
      value={prefs.appendSystemPrompt ?? ''}
      oninput={onTextInput('appendSystemPrompt')}
      placeholder="Additional instructions appended to the default system prompt"
      spellcheck="false"
    ></textarea>
  </div>
</div>

<ProfileEnvVarsSection
  customEnv={prefs.customEnv}
  hint="Extra env vars passed to Claude Code sessions in this profile"
  onChange={(v) => set('customEnv', v)}
/>

<div class="subsection">
  <h4 class="subsection-title">Advanced</h4>

  <div class="field">
    <label class="field-label" for="claude-settings">Settings JSON override</label>
    <textarea
      id="claude-settings"
      class="text-input textarea mono"
      rows="4"
      value={prefs.settingsJson ?? ''}
      oninput={onTextInput('settingsJson')}
      placeholder={'{"language": "japanese", "effortLevel": "high"}'}
      spellcheck="false"
    ></textarea>
    <span class="field-hint"
      >Merged into per-session settings.json. Hooks and status line are always preserved.</span
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
