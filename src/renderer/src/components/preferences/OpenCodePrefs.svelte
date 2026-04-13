<script lang="ts">
  import { prefs, setPref } from '../../lib/stores/preferences.svelte'

  // Model & behavior
  let model = $derived(prefs['opencode.model'] || '')

  // API
  let apiKey = $derived(prefs['opencode.apiKey'] || '')

  // Custom env vars
  let envEntries = $derived.by(() => {
    const raw = prefs['opencode.customEnv']
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
  let settingsJson = $derived(prefs['opencode.settingsJson'] || '')

  function persistEnvEntries(entries: Array<{ key: string; value: string }>): void {
    const obj: Record<string, string> = {}
    for (const entry of entries) {
      if (entry.key.trim()) obj[entry.key.trim()] = entry.value
    }
    if (Object.keys(obj).length > 0) {
      setPref('opencode.customEnv', JSON.stringify(obj))
    } else {
      setPref('opencode.customEnv', '')
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
  <h3 class="section-title">OpenCode</h3>

  <div class="subsection">
    <h4 class="subsection-title">Model & behavior</h4>

    <div class="field">
      <label class="field-label" for="opencode-model">Model</label>
      <input
        id="opencode-model"
        class="text-input"
        type="text"
        value={model}
        onchange={updatePref('opencode.model')}
        placeholder="provider/model"
        spellcheck="false"
      />
      <span class="field-hint"
        >Format: provider/model (e.g. anthropic/claude-sonnet-4-20250514)</span
      >
    </div>
  </div>

  <div class="subsection">
    <h4 class="subsection-title">API</h4>

    <div class="field">
      <label class="field-label" for="opencode-apikey">API key</label>
      <span class="field-hint"
        >Set as ANTHROPIC_API_KEY. For other providers, use environment variables below</span
      >
      <input
        id="opencode-apikey"
        class="text-input"
        type="password"
        value={apiKey}
        onchange={updatePref('opencode.apiKey')}
        placeholder="Uses existing env variable"
        spellcheck="false"
        autocomplete="off"
      />
    </div>
  </div>

  <div class="subsection">
    <h4 class="subsection-title">Environment variables</h4>

    <p class="field-hint" style="margin: 0 0 4px;">Extra env vars passed to OpenCode sessions</p>

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
          onkeydown={(e) => e.key === 'Enter' && addEnvVar()}
        />
        <input
          class="form-input"
          bind:value={newEnvValue}
          placeholder="value"
          spellcheck="false"
          onkeydown={(e) => e.key === 'Enter' && addEnvVar()}
        />
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
      <label class="field-label" for="opencode-settings">Config JSON override</label>
      <textarea
        id="opencode-settings"
        class="text-input textarea mono"
        rows="4"
        value={settingsJson}
        onchange={updatePref('opencode.settingsJson')}
        placeholder={'{"key": "value"}'}
        spellcheck="false"
      ></textarea>
      <span class="field-hint">Passed via OPENCODE_CONFIG_CONTENT at session start</span>
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
    filter: brightness(1.15);
  }

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
