<script lang="ts">
  import { prefs, setPref } from '../../lib/stores/preferences.svelte'
  import CustomSelect from '../shared/CustomSelect.svelte'

  // Model & behavior
  let model = $derived(prefs['claude.model'] || '')
  let permissionMode = $derived(prefs['claude.permissionMode'] || '')
  let effortLevel = $derived(prefs['claude.effortLevel'] || 'auto')

  // API / Provider
  let apiKey = $derived(prefs['claude.apiKey'] || '')
  let baseUrl = $derived(prefs['claude.baseUrl'] || '')
  let provider = $derived(prefs['claude.provider'] || '')

  // System prompt
  let appendSystemPrompt = $derived(prefs['claude.appendSystemPrompt'] || '')

  // Custom env vars
  let envEntries = $derived.by(() => {
    const raw = prefs['claude.customEnv']
    if (!raw) return [] as Array<{ key: string; value: string }>
    try {
      const parsed = JSON.parse(raw) as Record<string, string>
      return Object.entries(parsed).map(([key, value]) => ({ key, value }))
    } catch {
      return [] as Array<{ key: string; value: string }>
    }
  })
  let showEnvForm = $state(false)
  let newEnvKey = $state('')
  let newEnvValue = $state('')

  // Settings JSON
  let settingsJson = $derived(prefs['claude.settingsJson'] || '')

  function persistEnvEntries(entries: Array<{ key: string; value: string }>): void {
    const obj: Record<string, string> = {}
    for (const entry of entries) {
      if (entry.key.trim()) obj[entry.key.trim()] = entry.value
    }
    if (Object.keys(obj).length > 0) {
      setPref('claude.customEnv', JSON.stringify(obj))
    } else {
      setPref('claude.customEnv', '')
    }
  }

  function addEnvVar(): void {
    if (!newEnvKey.trim()) return
    persistEnvEntries([...envEntries, { key: newEnvKey.trim(), value: newEnvValue }])
    newEnvKey = ''
    newEnvValue = ''
    showEnvForm = false
  }

  function removeEnvVar(index: number): void {
    persistEnvEntries(envEntries.filter((_, i) => i !== index))
  }

  function updatePref(key: string): (e: Event) => void {
    return (e: Event) => {
      const target = e.target as HTMLInputElement | HTMLSelectElement | HTMLTextAreaElement
      setPref(key, target.value)
    }
  }
</script>

<div class="section">
  <h3 class="section-title">Claude Code</h3>

  <div class="subsection">
    <h4 class="subsection-title">Model & behavior</h4>

    <div class="field">
      <label class="field-label" for="claude-model">Model</label>
      <input
        id="claude-model"
        class="text-input"
        type="text"
        value={model}
        onchange={updatePref('claude.model')}
        placeholder="sonnet, opus, haiku, or model ID"
        spellcheck="false"
      />
    </div>

    <div class="field">
      <label class="field-label" for="claude-perm">Permission mode</label>
      <CustomSelect
        id="claude-perm"
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

    <div class="field">
      <label class="field-label" for="claude-effort">Effort level</label>
      <CustomSelect
        id="claude-effort"
        value={effortLevel}
        options={[
          { value: 'low', label: 'low' },
          { value: 'medium', label: 'medium' },
          { value: 'high', label: 'high' },
          { value: 'max', label: 'max' },
          { value: 'auto', label: 'auto' },
        ]}
        onchange={(v) => setPref('claude.effortLevel', v)}
      />
    </div>
  </div>

  <div class="subsection">
    <h4 class="subsection-title">API / Provider</h4>

    <div class="field">
      <label class="field-label" for="claude-apikey">API key</label>
      <input
        id="claude-apikey"
        class="text-input"
        type="password"
        value={apiKey}
        onchange={updatePref('claude.apiKey')}
        placeholder="sk-ant-..."
        spellcheck="false"
        autocomplete="off"
      />
    </div>

    <div class="field">
      <label class="field-label" for="claude-baseurl">Base URL</label>
      <input
        id="claude-baseurl"
        class="text-input"
        type="text"
        value={baseUrl}
        onchange={updatePref('claude.baseUrl')}
        placeholder="https://api.anthropic.com"
        spellcheck="false"
      />
    </div>

    <div class="field">
      <label class="field-label" for="claude-provider">Provider</label>
      <CustomSelect
        id="claude-provider"
        value={provider}
        options={[
          { value: '', label: 'Default (Anthropic)' },
          { value: 'bedrock', label: 'AWS Bedrock' },
          { value: 'vertex', label: 'Google Vertex AI' },
          { value: 'foundry', label: 'Microsoft Foundry' },
        ]}
        onchange={(v) => setPref('claude.provider', v)}
      />
    </div>
  </div>

  <div class="subsection">
    <h4 class="subsection-title">System prompt</h4>

    <div class="field">
      <label class="field-label" for="claude-sysprompt">Append to system prompt</label>
      <textarea
        id="claude-sysprompt"
        class="text-input textarea"
        rows="3"
        value={appendSystemPrompt}
        onchange={updatePref('claude.appendSystemPrompt')}
        placeholder="Additional instructions appended to the default system prompt"
        spellcheck="false"
      ></textarea>
    </div>
  </div>

  <div class="subsection">
    <h4 class="subsection-title">Environment variables</h4>

    {#if envEntries.length > 0}
      <div class="env-list">
        {#each envEntries as entry, i (entry.key)}
          <div class="env-row">
            <span class="env-key">{entry.key}</span>
            <span class="env-sep">=</span>
            <span class="env-value">{entry.value}</span>
            <button class="remove-btn" onclick={() => removeEnvVar(i)}>Remove</button>
          </div>
        {/each}
      </div>
    {/if}

    {#if showEnvForm}
      <div class="env-form">
        <input
          class="form-input"
          bind:value={newEnvKey}
          placeholder="VARIABLE_NAME"
          spellcheck="false"
        />
        <input class="form-input" bind:value={newEnvValue} placeholder="value" spellcheck="false" />
        <div class="form-actions">
          <button class="btn btn-cancel" onclick={() => (showEnvForm = false)}>Cancel</button>
          <button class="btn btn-add" onclick={addEnvVar}>Add</button>
        </div>
      </div>
    {:else}
      <button class="btn btn-add-item" onclick={() => (showEnvForm = true)}>+ Add variable</button>
    {/if}
  </div>

  <div class="subsection">
    <h4 class="subsection-title">Advanced</h4>

    <div class="field">
      <label class="field-label" for="claude-settings">Settings JSON override</label>
      <textarea
        id="claude-settings"
        class="text-input textarea mono"
        rows="4"
        value={settingsJson}
        onchange={updatePref('claude.settingsJson')}
        placeholder={'{"language": "japanese", "effortLevel": "high"}'}
        spellcheck="false"
      ></textarea>
      <span class="field-hint"
        >Merged into per-session settings.json. Hooks and status line are always preserved.</span
      >
    </div>
  </div>
</div>

<style>
  .section {
    display: flex;
    flex-direction: column;
    gap: 20px;
  }

  .section-title {
    font-size: 15px;
    font-weight: 600;
    color: var(--c-text);
    margin: 0;
  }

  .subsection {
    display: flex;
    flex-direction: column;
    gap: 10px;
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

  /* Env var list */
  .env-list {
    display: flex;
    flex-direction: column;
    gap: 4px;
  }

  .env-row {
    display: flex;
    align-items: center;
    gap: 6px;
    padding: 4px 10px;
    border-radius: 6px;
    background: var(--c-border-subtle);
    font-size: 13px;
  }

  .env-key {
    color: var(--c-accent-text);
    font-family: monospace;
    font-size: 12px;
  }

  .env-sep {
    color: var(--c-text-faint);
  }

  .env-value {
    color: var(--c-text-secondary);
    font-family: monospace;
    font-size: 12px;
    flex: 1;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .remove-btn {
    padding: 2px 8px;
    border: none;
    border-radius: 4px;
    background: var(--c-danger-bg);
    color: var(--c-danger-text);
    font-size: 11px;
    font-family: inherit;
    cursor: pointer;
    flex-shrink: 0;
  }

  .remove-btn:hover {
    background: var(--c-danger-bg);
  }

  /* Env form */
  .env-form {
    display: flex;
    flex-direction: column;
    gap: 8px;
    padding: 12px;
    border: 1px solid var(--c-border);
    border-radius: 8px;
    background: var(--c-border-subtle);
  }

  .form-input {
    padding: 6px 10px;
    border: 1px solid var(--c-border);
    border-radius: 6px;
    background: var(--c-hover);
    color: var(--c-text);
    font-size: 13px;
    font-family: monospace;
    outline: none;
  }

  .form-input:focus {
    border-color: var(--c-focus-ring);
  }

  .form-actions {
    display: flex;
    justify-content: flex-end;
    gap: 8px;
  }

  .btn {
    padding: 6px 14px;
    border-radius: 6px;
    font-size: 13px;
    font-family: inherit;
    cursor: pointer;
    border: none;
  }

  .btn-cancel {
    background: var(--c-active);
    color: var(--c-text);
  }

  .btn-add {
    background: var(--c-accent-bg);
    color: var(--c-accent-text);
  }

  .btn-add:hover {
    background: var(--c-accent-bg-hover);
  }

  .btn-add-item {
    align-self: flex-start;
    padding: 6px 14px;
    border: 1px dashed var(--c-text-faint);
    border-radius: 6px;
    background: transparent;
    color: var(--c-text-secondary);
    font-size: 13px;
    font-family: inherit;
    cursor: pointer;
  }

  .btn-add-item:hover {
    background: var(--c-hover);
    color: var(--c-text);
  }
</style>
