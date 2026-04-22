<script lang="ts">
  import type { ProfilePrefs } from '../../../../main/profiles/types'
  import CustomSelect from '../shared/CustomSelect.svelte'
  import ProfileEnvVarsSection from './ProfileEnvVarsSection.svelte'
  import {
    CLAUDE_PROVIDER_PRESETS,
    getClaudeProviderPreset,
    normalizeClaudeProviderPreset,
    type ClaudeProviderPresetId,
  } from '../../../../shared/claudeProviderPresets'

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

  const cloudProviderOptions = [
    { value: '', label: 'Default (Anthropic)' },
    { value: 'bedrock', label: 'AWS Bedrock' },
    { value: 'vertex', label: 'Google Vertex AI' },
    { value: 'foundry', label: 'Microsoft Foundry' },
  ]

  const presetOptions = CLAUDE_PROVIDER_PRESETS.map((preset) => ({
    value: preset.id,
    label: preset.label,
  }))

  let selectedPreset = $derived(getClaudeProviderPreset(prefs.claudeProviderPreset))
  let authLabel = $derived(
    selectedPreset.authEnv === 'ANTHROPIC_AUTH_TOKEN' ? 'Auth token' : 'API key',
  )
  let authHint = $derived.by(() => {
    if (selectedPreset.id === 'minimax') {
      return 'MiniMax token. Stored securely and injected as ANTHROPIC_AUTH_TOKEN.'
    }
    if (selectedPreset.id === 'zai') {
      return 'Z.AI API key. Stored securely and injected as ANTHROPIC_AUTH_TOKEN.'
    }
    if (selectedPreset.id === 'kimi') {
      return 'Kimi API key. Stored securely and injected as ANTHROPIC_API_KEY.'
    }
    return 'Anthropic API key. Falls back to ANTHROPIC_API_KEY env variable.'
  })
  let authPlaceholder = $derived.by(() => {
    if (hasApiKey) return '•••• (saved — leave empty to keep)'
    if (selectedPreset.id === 'kimi') return 'sk-kimi-...'
    return selectedPreset.authEnv === 'ANTHROPIC_AUTH_TOKEN' ? 'token-...' : 'sk-ant-...'
  })
  let customEnvHint = $derived.by(() => {
    if (selectedPreset.id === 'kimi') {
      return 'Extra env vars passed to Claude Code sessions. Optional workaround: ENABLE_TOOL_SEARCH=false.'
    }
    return 'Extra env vars passed to Claude Code sessions in this profile'
  })

  // Fetch the provider catalog so the effort field can flag models that
  // explicitly don't support reasoning (per models.dev `reasoning` flag).
  type CatalogModel = { value: string; label: string; family?: string | null; reasoning?: boolean }
  let providerModels = $state<CatalogModel[]>([])
  $effect(() => {
    const presetId = selectedPreset.id
    let cancelled = false
    window.api
      .getClaudeProviderModels(presetId)
      .then((options) => {
        if (!cancelled) providerModels = options
      })
      .catch(() => {})
    return () => {
      cancelled = true
    }
  })

  function catalogReasoningFor(value: string | undefined): boolean | null {
    const trimmed = (value ?? '').trim()
    if (trimmed === '') return null
    const direct = providerModels.find((m) => m.value === trimmed)
    if (direct) return direct.reasoning === true
    const alias = trimmed.toLowerCase()
    if (alias === 'haiku' || alias === 'sonnet' || alias === 'opus') {
      const resolved = providerModels.find(
        (m) =>
          m.family === alias ||
          m.family === `claude-${alias}` ||
          m.label.toLowerCase().includes(alias),
      )
      if (resolved) return resolved.reasoning === true
    }
    return null
  }

  let effortReasoningState = $derived(catalogReasoningFor(prefs.model))
  let effortGatedHint = $derived.by(() => {
    if (effortReasoningState === false) {
      return 'The selected model does not support reasoning effort; this value will be ignored.'
    }
    return null
  })

  function clearProviderModelFields(next: ProfilePrefs): void {
    next.providerModel = ''
    next.providerOpusModel = ''
    next.providerSonnetModel = ''
    next.providerHaikuModel = ''
  }

  function onPresetChange(value: string): void {
    const presetId = normalizeClaudeProviderPreset(value) as ClaudeProviderPresetId
    const preset = getClaudeProviderPreset(presetId)
    const next: ProfilePrefs = { ...prefs, claudeProviderPreset: presetId }

    next.baseUrl = preset.defaultBaseUrl ?? ''
    if (presetId !== 'anthropic') next.provider = ''
    clearProviderModelFields(next)

    if (presetId === 'minimax') {
      next.model = preset.defaultProviderModel ?? next.model ?? ''
    } else if (presetId === 'zai') {
      next.providerOpusModel = preset.defaultOpusModel ?? ''
      next.providerSonnetModel = preset.defaultSonnetModel ?? ''
      next.providerHaikuModel = preset.defaultHaikuModel ?? ''
    }

    onPrefsChange(next)
  }
</script>

<div class="subsection">
  <h4 class="subsection-title">Provider</h4>

  <div class="field">
    <label class="field-label" for="claude-provider-preset">Provider preset</label>
    <span class="field-hint"
      >Known Anthropic-compatible providers configured in the current profile.</span
    >
    <CustomSelect
      id="claude-provider-preset"
      value={selectedPreset.id}
      options={presetOptions}
      onchange={onPresetChange}
    />
  </div>

  {#if selectedPreset.id === 'anthropic'}
    <div class="field">
      <label class="field-label" for="claude-provider">Cloud provider</label>
      <span class="field-hint">Cloud provider for the Claude API backend</span>
      <CustomSelect
        id="claude-provider"
        value={prefs.provider ?? ''}
        options={cloudProviderOptions}
        onchange={(v) => set('provider', v)}
        maxWidth="260px"
      />
    </div>
  {/if}

  <div class="preset-card">
    <div class="preset-title-row">
      <div class="preset-title">{selectedPreset.label}</div>
      {#if selectedPreset.docsUrl}
        <a class="preset-link" href={selectedPreset.docsUrl} target="_blank" rel="noreferrer">
          Docs
        </a>
      {/if}
    </div>
    <div class="preset-description">{selectedPreset.description}</div>
    {#if selectedPreset.notes && selectedPreset.notes.length > 0}
      <div class="preset-notes">
        {#each selectedPreset.notes as note (note)}
          <div class="preset-note">{note}</div>
        {/each}
      </div>
    {/if}
  </div>
</div>

<div class="subsection">
  <h4 class="subsection-title">Model & behavior</h4>

  <div class="field">
    <label class="field-label" for="claude-model">Model</label>
    <span class="field-hint"
      >{selectedPreset.id === 'minimax'
        ? 'MiniMax model ID used for Claude aliases and direct model override.'
        : 'Short name (sonnet, opus, haiku) or full model ID'}</span
    >
    <input
      id="claude-model"
      class="text-input"
      type="text"
      value={prefs.model ?? ''}
      oninput={onTextInput('model')}
      placeholder={selectedPreset.id === 'minimax'
        ? (selectedPreset.defaultProviderModel ?? 'MiniMax-M2.7')
        : 'sonnet, opus, haiku, or model ID'}
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
    {#if effortGatedHint}
      <span class="field-warning">{effortGatedHint}</span>
    {/if}
  </div>
</div>

<div class="subsection">
  <h4 class="subsection-title">API</h4>

  <div class="field">
    <label class="field-label" for="claude-apikey">{authLabel}</label>
    <span class="field-hint">{authHint}</span>
    <input
      id="claude-apikey"
      class="text-input"
      type="password"
      value={apiKey}
      oninput={(e) => onApiKeyChange((e.target as HTMLInputElement).value)}
      placeholder={authPlaceholder}
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

  {#if selectedPreset.id === 'zai'}
    <div class="field-grid">
      <div class="field">
        <label class="field-label" for="claude-provider-opus">Opus mapping</label>
        <input
          id="claude-provider-opus"
          class="text-input"
          type="text"
          value={prefs.providerOpusModel ?? ''}
          oninput={onTextInput('providerOpusModel')}
          placeholder={selectedPreset.defaultOpusModel ?? 'GLM-4.7'}
          spellcheck="false"
        />
      </div>
      <div class="field">
        <label class="field-label" for="claude-provider-sonnet">Sonnet mapping</label>
        <input
          id="claude-provider-sonnet"
          class="text-input"
          type="text"
          value={prefs.providerSonnetModel ?? ''}
          oninput={onTextInput('providerSonnetModel')}
          placeholder={selectedPreset.defaultSonnetModel ?? 'GLM-4.7'}
          spellcheck="false"
        />
      </div>
      <div class="field">
        <label class="field-label" for="claude-provider-haiku">Haiku mapping</label>
        <input
          id="claude-provider-haiku"
          class="text-input"
          type="text"
          value={prefs.providerHaikuModel ?? ''}
          oninput={onTextInput('providerHaikuModel')}
          placeholder={selectedPreset.defaultHaikuModel ?? 'GLM-4.5-Air'}
          spellcheck="false"
        />
      </div>
    </div>
  {/if}
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
  hint={customEnvHint}
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

  .field-grid {
    display: grid;
    grid-template-columns: repeat(3, minmax(0, 1fr));
    gap: 10px;
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

  .field-warning {
    font-size: 11px;
    color: var(--c-warning-text, var(--c-accent-text));
    font-style: italic;
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

  .preset-card {
    display: flex;
    flex-direction: column;
    gap: 6px;
    padding: 10px 12px;
    border: 1px solid var(--c-border-subtle);
    border-radius: 6px;
    background: color-mix(in srgb, var(--c-bg) 90%, black);
  }

  .preset-title-row {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 8px;
  }

  .preset-title {
    font-size: 12.5px;
    font-weight: 600;
    color: var(--c-text);
  }

  .preset-link {
    color: var(--c-accent-text);
    font-size: 11.5px;
    text-decoration: none;
  }

  .preset-link:hover {
    text-decoration: underline;
  }

  .preset-description,
  .preset-note {
    font-size: 11.5px;
    color: var(--c-text-secondary);
    line-height: 1.45;
  }

  .preset-notes {
    display: flex;
    flex-direction: column;
    gap: 4px;
  }
</style>
