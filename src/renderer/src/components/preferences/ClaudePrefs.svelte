<script lang="ts">
  import { prefs, setPref } from '../../lib/stores/preferences.svelte'
  import CustomSelect from '../shared/CustomSelect.svelte'

  // Model & behavior
  let model = $derived(prefs['claude.model'] || '')
  let permissionMode = $derived(prefs['claude.permissionMode'] || '')
  let effortLevel = $derived(prefs['claude.effortLevel'] || '')

  // API / Provider
  let apiKey = $derived(prefs['claude.apiKey'] || '')
  let baseUrl = $derived(prefs['claude.baseUrl'] || '')
  let provider = $derived(prefs['claude.provider'] || '')

  // System prompt
  let appendSystemPrompt = $derived(prefs['claude.appendSystemPrompt'] || '')

  // Custom env vars
  let envEntries: Array<{ key: string; value: string }> = $state([])
  let showEnvForm = $state(false)
  let newEnvKey = $state('')
  let newEnvValue = $state('')

  // Settings JSON
  let settingsJson = $derived(prefs['claude.settingsJson'] || '')

  // Load env entries from stored JSON
  $effect(() => {
    const raw = prefs['claude.customEnv']
    if (raw) {
      try {
        const parsed = JSON.parse(raw) as Record<string, string>
        envEntries = Object.entries(parsed).map(([key, value]) => ({ key, value }))
      } catch {
        envEntries = []
      }
    } else {
      envEntries = []
    }
  })

  function saveEnvEntries(): void {
    const obj: Record<string, string> = {}
    for (const entry of envEntries) {
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
    envEntries = [...envEntries, { key: newEnvKey.trim(), value: newEnvValue }]
    newEnvKey = ''
    newEnvValue = ''
    showEnvForm = false
    saveEnvEntries()
  }

  function removeEnvVar(index: number): void {
    envEntries = envEntries.filter((_, i) => i !== index)
    saveEnvEntries()
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
          { value: '', label: 'Default' },
          { value: 'low', label: 'Low' },
          { value: 'medium', label: 'Medium' },
          { value: 'high', label: 'High' },
          { value: 'max', label: 'Max' },
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
    color: #e0e0e0;
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
    color: rgba(255, 255, 255, 0.35);
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
    color: rgba(255, 255, 255, 0.5);
  }

  .field-hint {
    font-size: 11px;
    color: rgba(255, 255, 255, 0.3);
  }

  .text-input {
    padding: 6px 10px;
    border: 1px solid rgba(255, 255, 255, 0.12);
    border-radius: 6px;
    background: rgba(255, 255, 255, 0.06);
    color: #e0e0e0;
    font-size: 13px;
    font-family: inherit;
    outline: none;
  }

  .text-input:focus {
    border-color: rgba(116, 192, 252, 0.5);
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
    background: rgba(255, 255, 255, 0.03);
    font-size: 13px;
  }

  .env-key {
    color: rgba(116, 192, 252, 0.8);
    font-family: monospace;
    font-size: 12px;
  }

  .env-sep {
    color: rgba(255, 255, 255, 0.25);
  }

  .env-value {
    color: rgba(255, 255, 255, 0.6);
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
    background: rgba(255, 100, 100, 0.15);
    color: rgba(255, 120, 120, 0.8);
    font-size: 11px;
    font-family: inherit;
    cursor: pointer;
    flex-shrink: 0;
  }

  .remove-btn:hover {
    background: rgba(255, 100, 100, 0.25);
  }

  /* Env form */
  .env-form {
    display: flex;
    flex-direction: column;
    gap: 8px;
    padding: 12px;
    border: 1px solid rgba(255, 255, 255, 0.1);
    border-radius: 8px;
    background: rgba(255, 255, 255, 0.03);
  }

  .form-input {
    padding: 6px 10px;
    border: 1px solid rgba(255, 255, 255, 0.12);
    border-radius: 6px;
    background: rgba(255, 255, 255, 0.06);
    color: #e0e0e0;
    font-size: 13px;
    font-family: monospace;
    outline: none;
  }

  .form-input:focus {
    border-color: rgba(116, 192, 252, 0.5);
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
    background: rgba(255, 255, 255, 0.08);
    color: rgba(255, 255, 255, 0.7);
  }

  .btn-add {
    background: rgba(116, 192, 252, 0.2);
    color: rgba(116, 192, 252, 0.9);
  }

  .btn-add:hover {
    background: rgba(116, 192, 252, 0.3);
  }

  .btn-add-item {
    align-self: flex-start;
    padding: 6px 14px;
    border: 1px dashed rgba(255, 255, 255, 0.15);
    border-radius: 6px;
    background: transparent;
    color: rgba(255, 255, 255, 0.5);
    font-size: 13px;
    font-family: inherit;
    cursor: pointer;
  }

  .btn-add-item:hover {
    background: rgba(255, 255, 255, 0.05);
    color: rgba(255, 255, 255, 0.7);
  }
</style>
