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

<div class="flex flex-col gap-2.5 mb-5">
  <h4 class="text-xs font-semibold uppercase tracking-[0.5px] text-text-muted m-0">{label}</h4>
  {#if hint}
    <span class="text-xs text-text-faint">{hint}</span>
  {/if}

  {#if envEntries.length > 0}
    <div class="flex flex-col gap-1">
      {#each envEntries as entry, i (entry.key)}
        {@const isRevealed = revealed.has(entry.key)}
        <div class="flex items-center gap-1.5 px-2.5 py-1 rounded-lg bg-border-subtle text-md">
          <span class="text-accent-text font-mono text-sm">{entry.key}</span>
          <span class="text-text-faint">=</span>
          <span
            class="font-mono text-sm flex-1 overflow-hidden text-ellipsis whitespace-nowrap {isRevealed
              ? 'text-text-secondary'
              : 'text-text-faint tracking-[1px]'}"
          >
            {isRevealed ? entry.value : maskValue(entry.value)}
          </span>
          <button
            class="px-2 py-0.5 border border-border rounded-md bg-transparent text-text-secondary text-xs font-inherit cursor-pointer flex-shrink-0 hover:bg-hover hover:text-text"
            onclick={() => toggleReveal(entry.key)}
            title={isRevealed ? 'Hide value' : 'Show value'}
            aria-label={isRevealed ? `Hide ${entry.key}` : `Show ${entry.key}`}
          >
            {isRevealed ? 'Hide' : 'Show'}
          </button>
          <button
            class="px-2 py-0.5 border-0 rounded-md bg-danger-bg text-danger-text text-xs font-inherit cursor-pointer flex-shrink-0"
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
    <div class="flex flex-col gap-2 p-3 border border-border rounded-xl bg-border-subtle">
      <input
        class="px-2.5 py-1.5 border border-border rounded-lg bg-hover text-text text-md font-mono outline-none focus:border-focus-ring"
        bind:value={newEnvKey}
        placeholder="VARIABLE_NAME"
        spellcheck="false"
        onkeydown={(e) => e.key === 'Enter' && addEnvVar()}
      />
      <input
        class="px-2.5 py-1.5 border border-border rounded-lg bg-hover text-text text-md font-mono outline-none focus:border-focus-ring"
        bind:value={newEnvValue}
        placeholder="value"
        spellcheck="false"
        onkeydown={(e) => e.key === 'Enter' && addEnvVar()}
      />
      <div class="flex justify-end gap-2">
        <button
          class="px-3.5 py-1.5 rounded-lg text-md font-inherit cursor-pointer border-0 bg-active text-text"
          onclick={() => (showEnvForm = false)}>Cancel</button
        >
        <button
          class="px-3.5 py-1.5 rounded-lg text-md font-inherit cursor-pointer border-0 bg-accent-bg text-accent-text"
          onclick={addEnvVar}>Add</button
        >
      </div>
    </div>
  {:else}
    <button
      class="self-start px-3.5 py-1.5 border border-dashed border-text-faint rounded-lg bg-transparent text-text-secondary text-md font-inherit cursor-pointer hover:bg-hover hover:text-text"
      onclick={() => (showEnvForm = true)}>+ Add variable</button
    >
  {/if}
</div>
