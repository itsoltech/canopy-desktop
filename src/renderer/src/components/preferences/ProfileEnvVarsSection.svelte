<script lang="ts">
  import { Plus, X, Eye, EyeOff } from '@lucide/svelte'
  import { SvelteSet } from 'svelte/reactivity'
  import { confirm } from '../../lib/stores/dialogs.svelte'
  import PrefsSection from './_partials/PrefsSection.svelte'

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
  let newValueRevealed = $state(false)

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
    newValueRevealed = false
    showEnvForm = false
  }

  function cancelAdd(): void {
    newEnvKey = ''
    newEnvValue = ''
    newValueRevealed = false
    showEnvForm = false
  }

  async function removeEnvVar(index: number): Promise<void> {
    const entry = envEntries[index]
    if (!entry) return
    const ok = await confirm({
      title: 'Remove variable',
      message: `Remove environment variable "${entry.key}"?`,
      details: 'The value is masked and cannot be recovered after removal.',
      confirmLabel: 'Remove',
      destructive: true,
    })
    if (!ok) return
    persistEnvEntries(envEntries.filter((_, i) => i !== index))
  }
</script>

<PrefsSection title={label} description={hint}>
  <div class="flex flex-col gap-2 py-3 border-t border-border-subtle first:border-t-0 first:pt-0">
    {#if envEntries.length > 0}
      <div class="flex flex-col gap-1">
        {#each envEntries as entry, i (entry.key)}
          {@const isRevealed = revealed.has(entry.key)}
          <div
            class="flex items-center gap-1.5 px-2.5 py-1 rounded-md bg-bg-input text-md border border-border-subtle"
          >
            <span class="text-accent-text font-mono text-sm shrink-0">{entry.key}</span>
            <span class="text-text-faint shrink-0">=</span>
            <span
              class="font-mono text-sm flex-1 truncate {isRevealed
                ? 'text-text-secondary'
                : 'text-text-faint tracking-wider'}"
            >
              {isRevealed ? entry.value : maskValue(entry.value)}
            </span>
            <button
              type="button"
              class="flex items-center justify-center size-6 rounded-md bg-transparent border-0 text-text-muted cursor-pointer shrink-0 hover:bg-hover hover:text-text"
              onclick={() => toggleReveal(entry.key)}
              title={isRevealed ? 'Hide value' : 'Show value'}
              aria-label={isRevealed ? `Hide ${entry.key}` : `Show ${entry.key}`}
            >
              {#if isRevealed}
                <EyeOff size={12} />
              {:else}
                <Eye size={12} />
              {/if}
            </button>
            <button
              type="button"
              class="flex items-center justify-center size-6 rounded-md bg-transparent border-0 text-text-muted cursor-pointer shrink-0 hover:bg-danger-bg hover:text-danger-text"
              onclick={() => removeEnvVar(i)}
              aria-label={`Remove environment variable ${entry.key}`}
              title="Remove"
            >
              <X size={12} />
            </button>
          </div>
        {/each}
      </div>
    {/if}

    {#if showEnvForm}
      <div class="flex flex-col gap-2 p-2.5 border border-border rounded-md bg-bg-input">
        <input
          class="px-2.5 py-1.5 border border-border rounded-md bg-bg text-text text-md font-mono outline-none focus:border-focus-ring placeholder:text-text-faint"
          name="newEnvKey"
          aria-label="Variable name"
          bind:value={newEnvKey}
          placeholder="VARIABLE_NAME"
          spellcheck="false"
          autocomplete="off"
          onkeydown={(e) => e.key === 'Enter' && addEnvVar()}
        />
        <div class="flex items-center gap-1.5">
          <input
            class="flex-1 min-w-0 px-2.5 py-1.5 border border-border rounded-md bg-bg text-text text-md font-mono outline-none focus:border-focus-ring placeholder:text-text-faint"
            type={newValueRevealed ? 'text' : 'password'}
            name="newEnvValue"
            aria-label="Variable value"
            bind:value={newEnvValue}
            placeholder="value"
            spellcheck="false"
            autocomplete="off"
            onkeydown={(e) => e.key === 'Enter' && addEnvVar()}
          />
          <button
            type="button"
            class="flex items-center justify-center size-7 rounded-md bg-transparent border border-border text-text-muted cursor-pointer shrink-0 hover:bg-hover hover:text-text"
            onclick={() => (newValueRevealed = !newValueRevealed)}
            title={newValueRevealed ? 'Hide value' : 'Show value'}
            aria-label={newValueRevealed ? 'Hide value' : 'Show value'}
          >
            {#if newValueRevealed}
              <EyeOff size={12} />
            {:else}
              <Eye size={12} />
            {/if}
          </button>
        </div>
        <div class="flex justify-end gap-2">
          <button
            type="button"
            class="px-3 py-1 rounded-md text-sm font-inherit cursor-pointer border border-border bg-transparent text-text-secondary hover:bg-hover hover:text-text"
            onclick={cancelAdd}>Cancel</button
          >
          <button
            type="button"
            class="px-3 py-1 rounded-md text-sm font-inherit cursor-pointer border-0 bg-accent-bg text-accent-text hover:bg-accent-bg-hover"
            onclick={addEnvVar}>Add</button
          >
        </div>
      </div>
    {:else}
      <button
        type="button"
        class="self-start flex items-center gap-1 px-2.5 py-1 rounded-md bg-border-subtle border border-border text-text-secondary text-sm font-inherit cursor-pointer hover:bg-active hover:text-text"
        onclick={() => (showEnvForm = true)}
      >
        <Plus size={12} />
        <span>Add variable</span>
      </button>
    {/if}
  </div>
</PrefsSection>
