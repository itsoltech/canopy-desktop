<script lang="ts">
  import { Plus, Trash2 } from '@lucide/svelte'

  interface ConfigEntry {
    name: string
    command: string
    args?: string
    cwd?: string
    env?: Record<string, string>
    pre_run?: string
    post_run?: string
  }

  let {
    config,
    isNew = false,
    saving = false,
    error = '',
    onSave,
  }: {
    config?: ConfigEntry
    isNew?: boolean
    saving?: boolean
    error?: string
    onSave: (config: ConfigEntry) => void
  } = $props()

  let name = $state('')
  let command = $state('')
  let args = $state('')
  let cwd = $state('')
  let preRun = $state('')
  let postRun = $state('')
  let envPairs = $state<{ key: string; value: string }[]>([])

  export function loadConfig(c: ConfigEntry): void {
    name = c.name
    command = c.command
    args = c.args ?? ''
    cwd = c.cwd ?? ''
    preRun = c.pre_run ?? ''
    postRun = c.post_run ?? ''
    envPairs = Object.entries(c.env ?? {}).map(([key, value]) => ({ key, value }))
  }

  export function resetForm(): void {
    name = ''
    command = ''
    args = ''
    cwd = ''
    preRun = ''
    postRun = ''
    envPairs = []
  }

  export function isDirty(original?: ConfigEntry): boolean {
    if (!original) return name.trim() !== '' || command.trim() !== ''
    return (
      name.trim() !== original.name ||
      command.trim() !== original.command ||
      (args.trim() || '') !== (original.args ?? '') ||
      (cwd.trim() || '') !== (original.cwd ?? '') ||
      (preRun.trim() || '') !== (original.pre_run ?? '') ||
      (postRun.trim() || '') !== (original.post_run ?? '')
    )
  }

  $effect(() => {
    if (config) loadConfig(config)
    else resetForm()
  })

  function addEnvPair(): void {
    envPairs = [...envPairs, { key: '', value: '' }]
  }

  function removeEnvPair(index: number): void {
    envPairs = envPairs.filter((_, i) => i !== index)
  }

  function buildConfig(): ConfigEntry | null {
    if (!name.trim() || !command.trim()) return null
    const env: Record<string, string> = {}
    for (const pair of envPairs) {
      if (pair.key.trim()) env[pair.key.trim()] = pair.value
    }
    return {
      name: name.trim(),
      command: command.trim(),
      ...(args.trim() ? { args: args.trim() } : {}),
      ...(cwd.trim() ? { cwd: cwd.trim() } : {}),
      ...(Object.keys(env).length > 0 ? { env } : {}),
      ...(preRun.trim() ? { pre_run: preRun.trim() } : {}),
      ...(postRun.trim() ? { post_run: postRun.trim() } : {}),
    }
  }

  function handleSubmit(): void {
    const result = buildConfig()
    if (!result) return
    onSave(result)
  }
</script>

<div class="form">
  <div class="field">
    <label for="rcf-name">Name</label>
    <input id="rcf-name" type="text" bind:value={name} placeholder="Dev Server" />
  </div>

  <div class="field">
    <label for="rcf-command">Command</label>
    <input id="rcf-command" type="text" bind:value={command} placeholder="npm" />
  </div>

  <div class="field">
    <label for="rcf-args">Arguments</label>
    <input id="rcf-args" type="text" bind:value={args} placeholder="run dev" />
  </div>

  <div class="field">
    <label for="rcf-cwd">Working Directory</label>
    <input id="rcf-cwd" type="text" bind:value={cwd} placeholder="relative to config location" />
  </div>

  <div class="section-label">
    <span>Environment Variables</span>
    <button class="add-btn" onclick={addEnvPair}>
      <Plus size={14} />
    </button>
  </div>

  {#each envPairs as pair, i (i)}
    <div class="env-row">
      <input type="text" bind:value={pair.key} placeholder="KEY" class="env-key" />
      <span class="env-eq">=</span>
      <input type="text" bind:value={pair.value} placeholder="value" class="env-value" />
      <button class="add-btn" onclick={() => removeEnvPair(i)}>
        <Trash2 size={12} />
      </button>
    </div>
  {/each}

  <div class="section-label"><span>Hooks</span></div>

  <div class="field">
    <label for="rcf-prerun">Pre-run</label>
    <input id="rcf-prerun" type="text" bind:value={preRun} placeholder="npm install" />
  </div>

  <div class="field">
    <label for="rcf-postrun">Post-run</label>
    <input id="rcf-postrun" type="text" bind:value={postRun} placeholder="echo done" />
  </div>

  {#if error}
    <div class="error">{error}</div>
  {/if}

  <div class="form-footer">
    <button
      class="btn primary"
      onclick={handleSubmit}
      disabled={saving || !name.trim() || !command.trim()}
    >
      {saving ? 'Saving...' : isNew ? 'Create' : 'Save'}
    </button>
  </div>
</div>

<style>
  .form {
    display: flex;
    flex-direction: column;
    gap: 10px;
  }

  .field {
    display: flex;
    flex-direction: column;
    gap: 4px;
  }

  .field label {
    font-size: 11px;
    font-weight: 500;
    color: var(--c-text-muted);
  }

  .field input {
    height: 30px;
    padding: 0 10px;
    border: 1px solid var(--c-border);
    border-radius: 6px;
    background: var(--c-bg-secondary);
    color: var(--c-text);
    font-size: 13px;
    font-family: inherit;
    outline: none;
  }

  .field input:focus {
    border-color: var(--c-focus-ring);
  }

  .section-label {
    display: flex;
    align-items: center;
    justify-content: space-between;
    margin-top: 4px;
    font-size: 11px;
    font-weight: 600;
    color: var(--c-text-muted);
    text-transform: uppercase;
    letter-spacing: 0.5px;
  }

  .add-btn {
    display: flex;
    align-items: center;
    justify-content: center;
    width: 22px;
    height: 22px;
    border: none;
    background: none;
    color: var(--c-text-muted);
    cursor: pointer;
    border-radius: 4px;
  }

  .add-btn:hover {
    background: var(--c-hover);
    color: var(--c-text);
  }

  .env-row {
    display: flex;
    align-items: center;
    gap: 4px;
  }

  .env-key {
    flex: 1;
    height: 28px;
    padding: 0 8px;
    border: 1px solid var(--c-border);
    border-radius: 6px;
    background: var(--c-bg-secondary);
    color: var(--c-text);
    font-size: 12px;
    font-family: var(--font-mono, monospace);
    outline: none;
  }

  .env-eq {
    color: var(--c-text-muted);
    font-size: 12px;
  }

  .env-value {
    flex: 2;
    height: 28px;
    padding: 0 8px;
    border: 1px solid var(--c-border);
    border-radius: 6px;
    background: var(--c-bg-secondary);
    color: var(--c-text);
    font-size: 12px;
    font-family: var(--font-mono, monospace);
    outline: none;
  }

  .env-key:focus,
  .env-value:focus {
    border-color: var(--c-focus-ring);
  }

  .error {
    padding: 8px 10px;
    background: var(--c-danger-bg);
    color: var(--c-danger-text);
    border-radius: 6px;
    font-size: 12px;
  }

  .form-footer {
    display: flex;
    justify-content: flex-end;
    margin-top: 4px;
  }

  .btn {
    height: 30px;
    padding: 0 16px;
    border: none;
    border-radius: 6px;
    font-size: 13px;
    font-family: inherit;
    font-weight: 500;
    cursor: pointer;
  }

  .btn.primary {
    background: var(--c-accent-bg);
    color: var(--c-accent-text);
  }

  .btn.primary:hover {
    background: var(--c-accent-muted);
  }

  .btn:disabled {
    opacity: 0.4;
    cursor: default;
  }
</style>
