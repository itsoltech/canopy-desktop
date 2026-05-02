<script lang="ts">
  import CustomSelect from '../../shared/CustomSelect.svelte'

  interface ToolDraft {
    id: string
    name: string
    command: string
    args: string
    category: string
  }

  let {
    draft = $bindable(),
    mode,
    error,
    onCancel,
    onSubmit,
  }: {
    draft: ToolDraft
    mode: 'add' | 'edit'
    error: string
    onCancel: () => void
    onSubmit: () => void
  } = $props()

  const categoryOptions = [
    { value: 'ai', label: 'AI' },
    { value: 'git', label: 'Git' },
    { value: 'system', label: 'System' },
    { value: 'shell', label: 'Shell' },
  ]

  const submitLabel = $derived(mode === 'add' ? 'Add tool' : 'Save')
</script>

<form
  class="flex flex-col gap-2 p-3 border border-border rounded-md bg-bg-input"
  class:my-1={mode === 'edit'}
  class:mt-3={mode === 'add'}
  onsubmit={(e) => {
    e.preventDefault()
    onSubmit()
  }}
>
  <!-- svelte-ignore a11y_no_static_element_interactions -->
  <div
    class="contents"
    onkeydown={(e) => {
      if (e.key === 'Escape') onCancel()
    }}
  >
    {#if mode === 'edit'}
      <div class="flex items-center gap-2 text-2xs uppercase tracking-caps-tight text-text-faint">
        <span>Editing</span>
        <code class="font-mono text-text-muted normal-case tracking-normal">{draft.id}</code>
      </div>
    {:else}
      <input
        class="px-2.5 py-1.5 border border-border rounded-md bg-bg text-text text-md font-mono outline-none focus:border-focus-ring placeholder:text-text-faint"
        name="toolId"
        aria-label="Tool ID"
        bind:value={draft.id}
        placeholder="ID (e.g. my-tool)"
        spellcheck="false"
      />
    {/if}
    <input
      class="px-2.5 py-1.5 border border-border rounded-md bg-bg text-text text-md font-inherit outline-none focus:border-focus-ring placeholder:text-text-faint"
      name="toolName"
      aria-label="Display name"
      bind:value={draft.name}
      placeholder="Display name"
      spellcheck="false"
    />
    <input
      class="px-2.5 py-1.5 border border-border rounded-md bg-bg text-text text-md font-mono outline-none focus:border-focus-ring placeholder:text-text-faint"
      name="toolCommand"
      aria-label="Command"
      bind:value={draft.command}
      placeholder="Command (binary name)"
      spellcheck="false"
    />
    <input
      class="px-2.5 py-1.5 border border-border rounded-md bg-bg text-text text-md font-mono outline-none focus:border-focus-ring placeholder:text-text-faint"
      name="toolArgs"
      aria-label="Args"
      bind:value={draft.args}
      placeholder="Args (comma-separated)"
      spellcheck="false"
    />
    <CustomSelect
      value={draft.category}
      options={categoryOptions}
      maxWidth="100%"
      onchange={(v) => (draft.category = v)}
    />
    {#if error}
      <p class="text-sm text-danger-text m-0">{error}</p>
    {/if}
    <div class="flex justify-end gap-2">
      <button
        type="button"
        class="px-3 py-1 rounded-md text-sm font-inherit cursor-pointer border border-border bg-transparent text-text-secondary hover:bg-hover hover:text-text"
        onclick={onCancel}>Cancel</button
      >
      <button
        type="submit"
        class="px-3 py-1 rounded-md text-sm font-inherit cursor-pointer border-0 bg-accent-bg text-accent-text hover:bg-accent-bg-hover"
        >{submitLabel}</button
      >
    </div>
  </div>
</form>
