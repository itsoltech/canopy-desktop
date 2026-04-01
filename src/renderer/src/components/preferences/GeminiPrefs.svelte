<script lang="ts">
  import { prefs, setPref } from '../../lib/stores/preferences.svelte'
  import CustomSelect from '../shared/CustomSelect.svelte'

  // Model & behavior
  let model = $derived(prefs['gemini.model'] || '')
  let approvalMode = $derived(prefs['gemini.approvalMode'] || '')

  // API
  let apiKey = $derived(prefs['gemini.apiKey'] || '')

  // Custom env vars
  let envEntries = $derived.by(() => {
    const raw = prefs['gemini.customEnv']
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
  let settingsJson = $derived(prefs['gemini.settingsJson'] || '')

  function persistEnvEntries(entries: Array<{ key: string; value: string }>): void {
    const obj: Record<string, string> = {}
    for (const entry of entries) {
      if (entry.key.trim()) obj[entry.key.trim()] = entry.value
    }
    if (Object.keys(obj).length > 0) {
      setPref('gemini.customEnv', JSON.stringify(obj))
    } else {
      setPref('gemini.customEnv', '')
    }
  }

  function addEnvVar(): void {
    if (!newEnvKey.trim()) return
    persistEnvEntries([...envEntries, { key: newEnvKey.trim(), value: newEnvValue }])
    newEnvKey = ''
    newEnvValue = ''
    showEnvForm = false
  }

  function removeEnvVar(key: string): void {
    persistEnvEntries(envEntries.filter((e) => e.key !== key))
  }
</script>

<div class="section">
  <h3>Model & behavior</h3>
  <label class="field">
    <span class="field-label">Model</span>
    <input
      type="text"
      class="field-input"
      value={model}
      placeholder="Default"
      oninput={(e) => setPref('gemini.model', e.currentTarget.value)}
    />
    <span class="field-hint">Leave empty for Gemini CLI default</span>
  </label>

  <label class="field">
    <span class="field-label">Approval mode</span>
    <CustomSelect
      value={approvalMode}
      options={[
        { value: '', label: 'Default' },
        { value: 'default', label: 'Prompt' },
        { value: 'auto_edit', label: 'Auto Edit' },
        { value: 'yolo', label: 'YOLO' },
        { value: 'plan', label: 'Plan (read-only)' },
      ]}
      onChange={(v) => setPref('gemini.approvalMode', v)}
    />
  </label>
</div>

<div class="section">
  <h3>API</h3>
  <label class="field">
    <span class="field-label">API key</span>
    <input
      type="password"
      class="field-input"
      value={apiKey}
      placeholder="Uses GEMINI_API_KEY from environment"
      oninput={(e) => setPref('gemini.apiKey', e.currentTarget.value)}
    />
  </label>
</div>

<div class="section">
  <h3>Environment variables</h3>
  <p class="field-hint">Extra env vars passed to Gemini CLI sessions</p>
  {#if envEntries.length > 0}
    <div class="env-list">
      {#each envEntries as entry (entry.key)}
        <div class="env-row">
          <code class="env-key">{entry.key}</code>
          <span class="env-eq">=</span>
          <code class="env-val">{entry.value}</code>
          <button class="env-remove" onclick={() => removeEnvVar(entry.key)}>&times;</button>
        </div>
      {/each}
    </div>
  {/if}
  {#if showEnvForm}
    <div class="env-form">
      <input
        class="field-input env-input"
        placeholder="KEY"
        bind:value={newEnvKey}
        onkeydown={(e) => e.key === 'Enter' && addEnvVar()}
      />
      <span class="env-eq">=</span>
      <input
        class="field-input env-input"
        placeholder="value"
        bind:value={newEnvValue}
        onkeydown={(e) => e.key === 'Enter' && addEnvVar()}
      />
      <button class="btn btn-sm" onclick={addEnvVar}>Add</button>
      <button class="btn btn-sm btn-ghost" onclick={() => (showEnvForm = false)}>Cancel</button>
    </div>
  {:else}
    <button class="btn btn-sm" onclick={() => (showEnvForm = true)}>Add variable</button>
  {/if}
</div>

<div class="section">
  <h3>Advanced</h3>
  <label class="field">
    <span class="field-label">Settings JSON override</span>
    <textarea
      class="field-textarea"
      rows="4"
      value={settingsJson}
      placeholder={'{"key": "value"}'}
      oninput={(e) => setPref('gemini.settingsJson', e.currentTarget.value)}
    ></textarea>
    <span class="field-hint"
      >Merged into per-session .gemini/settings.json (hooks always preserved)</span
    >
  </label>
</div>

<style>
  .section {
    margin-bottom: 24px;
  }

  h3 {
    font-size: 13px;
    font-weight: 600;
    color: var(--c-text);
    margin: 0 0 12px;
  }

  .field {
    display: flex;
    flex-direction: column;
    gap: 4px;
    margin-bottom: 12px;
  }

  .field-label {
    font-size: 12px;
    color: var(--c-text-secondary);
  }

  .field-input {
    background: var(--c-hover);
    border: 1px solid var(--c-border);
    border-radius: 4px;
    padding: 6px 8px;
    color: var(--c-text);
    font-size: 13px;
    font-family: inherit;
  }

  .field-textarea {
    background: var(--c-hover);
    border: 1px solid var(--c-border);
    border-radius: 4px;
    padding: 6px 8px;
    color: var(--c-text);
    font-size: 12px;
    font-family: 'SF Mono', 'Menlo', monospace;
    resize: vertical;
  }

  .field-hint {
    font-size: 11px;
    color: var(--c-text-faint);
  }

  .env-list {
    display: flex;
    flex-direction: column;
    gap: 4px;
    margin-bottom: 8px;
  }

  .env-row {
    display: flex;
    align-items: center;
    gap: 4px;
    font-size: 12px;
  }

  .env-key {
    color: var(--c-text-secondary);
  }

  .env-eq {
    color: var(--c-text-faint);
  }

  .env-val {
    color: var(--c-text-secondary);
    flex: 1;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .env-remove {
    background: none;
    border: none;
    color: var(--c-danger-text);
    cursor: pointer;
    font-size: 14px;
    padding: 0 4px;
  }

  .env-form {
    display: flex;
    align-items: center;
    gap: 4px;
    margin-bottom: 8px;
  }

  .env-input {
    flex: 1;
    min-width: 0;
  }

  .btn {
    background: var(--c-active);
    border: 1px solid var(--c-border);
    border-radius: 4px;
    color: var(--c-text);
    cursor: pointer;
    font-size: 12px;
    padding: 4px 10px;
  }

  .btn-sm {
    font-size: 11px;
    padding: 3px 8px;
  }

  .btn-ghost {
    background: none;
    border: none;
  }
</style>
