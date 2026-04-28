<script lang="ts">
  import { SvelteSet } from 'svelte/reactivity'
  import { confirm } from '../../lib/stores/dialogs.svelte'

  let {
    customEnv,
    label = 'Environment variables',
    hint,
    onChange,
  }: {
    customEnv: string | undefined
    label?: string
    hint?: string
    onChange: (next: string) => void
  } = $props()

  let envEntries = $derived.by(() => {
    if (!customEnv) return [] as Array<{ key: string; value: string }>
    try {
      const parsed = JSON.parse(customEnv) as Record<string, string>
      return Object.entries(parsed).map(([key, value]) => ({ key, value }))
    } catch {
      return [] as Array<{ key: string; value: string }>
    }
  })

  let showEnvForm = $state(false)
  let newEnvKey = $state('')
  let newEnvValue = $state('')

  // Env var values often contain secrets (tokens, API keys). Mask by default
  // so they're not exposed in screenshots or over-the-shoulder views; users
  // can reveal individual rows on demand.
  const revealed = new SvelteSet<string>()
  function toggleReveal(key: string): void {
    if (revealed.has(key)) revealed.delete(key)
    else revealed.add(key)
  }
  function maskValue(value: string): string {
    if (value.length === 0) return ''
    return '•'.repeat(Math.min(value.length, 12))
  }

  function persistEnvEntries(entries: Array<{ key: string; value: string }>): void {
    const obj: Record<string, string> = {}
    for (const entry of entries) {
      if (entry.key.trim()) obj[entry.key.trim()] = entry.value
    }
    onChange(Object.keys(obj).length > 0 ? JSON.stringify(obj) : '')
  }

  function addEnvVar(): void {
    if (!newEnvKey.trim()) return
    persistEnvEntries([...envEntries, { key: newEnvKey.trim(), value: newEnvValue }])
    newEnvKey = ''
    newEnvValue = ''
    showEnvForm = false
  }

  async function removeEnvVar(index: number): Promise<void> {
    const entry = envEntries[index]
    if (!entry) return
    const ok = await confirm({
      title: 'Remove Variable',
      message: `Remove environment variable "${entry.key}"?`,
      details: 'The value is masked and cannot be recovered after removal.',
      confirmLabel: 'Remove',
      destructive: true,
    })
    if (!ok) return
    persistEnvEntries(envEntries.filter((_, i) => i !== index))
  }
</script>

<div class="subsection">
  <h4 class="subsection-title">{label}</h4>
  {#if hint}
    <span class="field-hint">{hint}</span>
  {/if}

  {#if envEntries.length > 0}
    <div class="env-list">
      {#each envEntries as entry, i (entry.key)}
        {@const isRevealed = revealed.has(entry.key)}
        <div class="env-row">
          <span class="env-key">{entry.key}</span>
          <span class="env-sep">=</span>
          <span class="env-value" class:masked={!isRevealed}>
            {isRevealed ? entry.value : maskValue(entry.value)}
          </span>
          <button
            class="reveal-btn"
            onclick={() => toggleReveal(entry.key)}
            title={isRevealed ? 'Hide value' : 'Show value'}
            aria-label={isRevealed ? `Hide ${entry.key}` : `Show ${entry.key}`}
          >
            {isRevealed ? 'Hide' : 'Show'}
          </button>
          <button
            class="remove-btn"
            onclick={() => removeEnvVar(i)}
            aria-label={`Remove environment variable ${entry.key}`}
          >
            Remove
          </button>
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
    color: var(--color-text-muted);
    margin: 0;
  }

  .field-hint {
    font-size: 11px;
    color: var(--color-text-faint);
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
    background: var(--color-border-subtle);
    font-size: 13px;
  }

  .env-key {
    color: var(--color-accent-text);
    font-family: monospace;
    font-size: 12px;
  }

  .env-sep {
    color: var(--color-text-faint);
  }

  .env-value {
    color: var(--color-text-secondary);
    font-family: monospace;
    font-size: 12px;
    flex: 1;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .env-value.masked {
    letter-spacing: 1px;
    color: var(--color-text-faint);
  }

  .reveal-btn {
    padding: 2px 8px;
    border: 1px solid var(--color-border);
    border-radius: 4px;
    background: transparent;
    color: var(--color-text-secondary);
    font-size: 11px;
    font-family: inherit;
    cursor: pointer;
    flex-shrink: 0;
  }

  .reveal-btn:hover {
    background: var(--color-hover);
    color: var(--color-text);
  }

  .remove-btn {
    padding: 2px 8px;
    border: none;
    border-radius: 4px;
    background: var(--color-danger-bg);
    color: var(--color-danger-text);
    font-size: 11px;
    font-family: inherit;
    cursor: pointer;
    flex-shrink: 0;
  }

  .env-form {
    display: flex;
    flex-direction: column;
    gap: 8px;
    padding: 12px;
    border: 1px solid var(--color-border);
    border-radius: 8px;
    background: var(--color-border-subtle);
  }

  .form-input {
    padding: 6px 10px;
    border: 1px solid var(--color-border);
    border-radius: 6px;
    background: var(--color-hover);
    color: var(--color-text);
    font-size: 13px;
    font-family: monospace;
    outline: none;
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
    background: var(--color-active);
    color: var(--color-text);
  }

  .btn-add {
    background: var(--color-accent-bg);
    color: var(--color-accent-text);
  }

  .btn-add-item {
    align-self: flex-start;
    padding: 6px 14px;
    border: 1px dashed var(--color-text-faint);
    border-radius: 6px;
    background: transparent;
    color: var(--color-text-secondary);
    font-size: 13px;
    font-family: inherit;
    cursor: pointer;
  }

  .btn-add-item:hover {
    background: var(--color-hover);
    color: var(--color-text);
  }
</style>
