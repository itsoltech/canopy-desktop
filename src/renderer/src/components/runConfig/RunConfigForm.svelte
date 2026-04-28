<script lang="ts">
  import { Plus, Trash2 } from '@lucide/svelte'

  interface ConfigEntry {
    name: string
    command: string
    args?: string
    cwd?: string
    max_instances?: number
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
  let maxInstances = $state(0)
  let preRun = $state('')
  let postRun = $state('')
  let envPairs = $state<{ key: string; value: string }[]>([])

  export function loadConfig(c: ConfigEntry): void {
    name = c.name
    command = c.command
    args = c.args ?? ''
    cwd = c.cwd ?? ''
    maxInstances = c.max_instances ?? 0
    preRun = c.pre_run ?? ''
    postRun = c.post_run ?? ''
    envPairs = Object.entries(c.env ?? {}).map(([key, value]) => ({ key, value }))
  }

  export function resetForm(): void {
    name = ''
    command = ''
    args = ''
    cwd = ''
    maxInstances = 0
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
      ...(maxInstances > 0 ? { max_instances: maxInstances } : {}),
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

  const inputCls =
    'h-[30px] px-2.5 border border-border rounded-lg bg-bg-secondary text-text text-md font-inherit outline-none focus:border-focus-ring'
  const fieldCls = 'flex flex-col gap-1'
  const labelCls = 'text-xs font-medium text-text-muted'
  const sectionLabelCls =
    'flex items-center justify-between mt-1 text-xs font-semibold text-text-muted uppercase tracking-[0.5px]'
  const addBtnCls =
    'flex items-center justify-center w-[22px] h-[22px] border-0 bg-transparent text-text-muted cursor-pointer rounded-md hover:bg-hover hover:text-text'
  const envInputCls =
    'h-7 px-2 border border-border rounded-lg bg-bg-secondary text-text text-sm font-mono outline-none focus:border-focus-ring'
</script>

<div class="flex flex-col gap-2.5">
  <div class={fieldCls}>
    <label class={labelCls} for="rcf-name">Name</label>
    <input id="rcf-name" type="text" class={inputCls} bind:value={name} placeholder="Dev Server" />
  </div>

  <div class={fieldCls}>
    <label class={labelCls} for="rcf-command">Command</label>
    <input id="rcf-command" type="text" class={inputCls} bind:value={command} placeholder="npm" />
  </div>

  <div class={fieldCls}>
    <label class={labelCls} for="rcf-args">Arguments</label>
    <input id="rcf-args" type="text" class={inputCls} bind:value={args} placeholder="run dev" />
  </div>

  <div class={fieldCls}>
    <label class={labelCls} for="rcf-cwd">Working Directory</label>
    <input
      id="rcf-cwd"
      type="text"
      class={inputCls}
      bind:value={cwd}
      placeholder="relative to config location"
    />
  </div>

  <div class={fieldCls}>
    <label class={labelCls} for="rcf-max">Max Instances</label>
    <input
      id="rcf-max"
      type="number"
      class={inputCls}
      min="0"
      bind:value={maxInstances}
      placeholder="0 = unlimited"
    />
  </div>

  <div class={sectionLabelCls}>
    <span>Environment Variables</span>
    <button class={addBtnCls} onclick={addEnvPair}>
      <Plus size={14} />
    </button>
  </div>

  {#each envPairs as pair, i (i)}
    <div class="flex items-center gap-1">
      <input type="text" bind:value={pair.key} placeholder="KEY" class="flex-1 {envInputCls}" />
      <span class="text-text-muted text-sm">=</span>
      <input
        type="text"
        bind:value={pair.value}
        placeholder="value"
        class="flex-[2] {envInputCls}"
      />
      <button class={addBtnCls} onclick={() => removeEnvPair(i)}>
        <Trash2 size={12} />
      </button>
    </div>
  {/each}

  <div class={sectionLabelCls}><span>Hooks</span></div>

  <div class={fieldCls}>
    <label class={labelCls} for="rcf-prerun">Pre-run</label>
    <input
      id="rcf-prerun"
      type="text"
      class={inputCls}
      bind:value={preRun}
      placeholder="npm install"
    />
  </div>

  <div class={fieldCls}>
    <label class={labelCls} for="rcf-postrun">Post-run</label>
    <input
      id="rcf-postrun"
      type="text"
      class={inputCls}
      bind:value={postRun}
      placeholder="echo done"
    />
  </div>

  {#if error}
    <div class="px-2.5 py-2 bg-danger-bg text-danger-text rounded-lg text-sm">{error}</div>
  {/if}

  <div class="flex justify-end mt-1">
    <button
      class="h-[30px] px-4 border-0 rounded-lg text-md font-medium font-inherit cursor-pointer bg-accent-bg text-accent-text enabled:hover:bg-accent-muted disabled:opacity-40 disabled:cursor-default"
      onclick={handleSubmit}
      disabled={saving || !name.trim() || !command.trim()}
    >
      {saving ? 'Saving...' : isNew ? 'Create' : 'Save'}
    </button>
  </div>
</div>
