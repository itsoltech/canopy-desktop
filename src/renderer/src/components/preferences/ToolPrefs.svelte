<script lang="ts">
  import { getTools } from '../../lib/stores/tools.svelte'
  import { confirm } from '../../lib/stores/dialogs.svelte'
  import ToolIcon from '../shared/ToolIcon.svelte'
  import CustomSelect from '../shared/CustomSelect.svelte'

  let showForm = $state(false)
  let newId = $state('')
  let newName = $state('')
  let newCommand = $state('')
  let newArgs = $state('')
  let newCategory = $state('system')
  let error = $state('')

  let editingId: string | null = $state(null)
  let editName = $state('')
  let editCommand = $state('')
  let editArgs = $state('')
  let editCategory = $state('')
  let editError = $state('')

  async function addTool(): Promise<void> {
    if (!newId.trim() || !newName.trim() || !newCommand.trim()) {
      error = 'ID, name, and command are required'
      return
    }
    if (getTools().some((t) => t.id === newId.trim())) {
      error = 'Tool ID already exists'
      return
    }

    try {
      await window.api.addCustomTool({
        id: newId.trim(),
        name: newName.trim(),
        command: newCommand.trim(),
        args: newArgs
          .split(',')
          .map((s) => s.trim())
          .filter(Boolean),
        category: newCategory,
      })
      newId = ''
      newName = ''
      newCommand = ''
      newArgs = ''
      newCategory = 'system'
      showForm = false
      error = ''
    } catch (e) {
      error = e instanceof Error ? e.message : String(e)
    }
  }

  async function removeTool(id: string, name: string): Promise<void> {
    const ok = await confirm({
      title: 'Remove Tool',
      message: `Remove tool "${name}"? This cannot be undone.`,
      confirmLabel: 'Remove',
      destructive: true,
    })
    if (!ok) return
    await window.api.removeCustomTool(id)
  }

  async function startEdit(tool: {
    id: string
    name: string
    command: string
    args: string[]
    category: string
  }): Promise<void> {
    editingId = tool.id
    editName = tool.name
    editCommand = tool.command
    editArgs = tool.args.join(', ')
    editCategory = tool.category
    editError = ''
  }

  function cancelEdit(): void {
    editingId = null
    editError = ''
  }

  async function saveEdit(): Promise<void> {
    if (!editingId) return
    if (!editName.trim() || !editCommand.trim()) {
      editError = 'Name and command are required'
      return
    }

    try {
      await window.api.updateCustomTool(editingId, {
        name: editName.trim(),
        command: editCommand.trim(),
        args: editArgs
          .split(',')
          .map((s) => s.trim())
          .filter(Boolean),
        category: editCategory,
      })
      editingId = null
      editError = ''
    } catch (e) {
      editError = e instanceof Error ? e.message : String(e)
    }
  }
</script>

<div class="flex flex-col gap-4">
  <h3 class="text-[15px] font-semibold text-text m-0">Tools</h3>
  <p class="text-sm text-text-secondary m-0">
    Register custom CLI tools that appear in the command palette and can be opened as tabs.
  </p>

  <div class="flex flex-col gap-1">
    {#each getTools() as tool (tool.id)}
      {#if editingId === tool.id}
        <!-- svelte-ignore a11y_no_static_element_interactions -->
        <div
          class="flex flex-col gap-2 p-3 border border-border rounded-xl bg-border-subtle w-full"
          onkeydown={(e) => {
            if (e.key === 'Enter') {
              e.preventDefault()
              saveEdit()
            }
            if (e.key === 'Escape') cancelEdit()
          }}
        >
          <div class="mb-1">
            <span class="text-xs text-text-faint font-mono">ID: {tool.id}</span>
          </div>
          <input
            class="px-2.5 py-1.5 border border-border rounded-lg bg-hover text-text text-md font-inherit outline-none focus:border-focus-ring"
            bind:value={editName}
            placeholder="Display name"
          />
          <input
            class="px-2.5 py-1.5 border border-border rounded-lg bg-hover text-text text-md font-inherit outline-none focus:border-focus-ring"
            bind:value={editCommand}
            placeholder="Command (binary name)"
          />
          <input
            class="px-2.5 py-1.5 border border-border rounded-lg bg-hover text-text text-md font-inherit outline-none focus:border-focus-ring"
            bind:value={editArgs}
            placeholder="Args (comma-separated)"
          />
          <CustomSelect
            value={editCategory}
            options={[
              { value: 'ai', label: 'AI' },
              { value: 'git', label: 'Git' },
              { value: 'system', label: 'System' },
              { value: 'shell', label: 'Shell' },
            ]}
            maxWidth="100%"
            onchange={(v) => (editCategory = v)}
          />
          {#if editError}
            <p class="text-sm text-danger m-0">{editError}</p>
          {/if}
          <div class="flex justify-end gap-2">
            <button
              class="px-3.5 py-1.5 rounded-lg text-md font-inherit cursor-pointer border-0 bg-active text-text"
              onclick={cancelEdit}>Cancel</button
            >
            <button
              class="px-3.5 py-1.5 rounded-lg text-md font-inherit cursor-pointer border-0 bg-accent-bg text-accent-text hover:bg-accent-bg-hover"
              onclick={saveEdit}>Save</button
            >
          </div>
        </div>
      {:else}
        <div class="flex items-center gap-2.5 px-2.5 py-1.5 rounded-lg bg-border-subtle text-md">
          <ToolIcon icon={tool.icon} size={16} />
          <span class="text-text min-w-[100px]">{tool.name}</span>
          <span class="text-text-secondary font-mono text-sm flex-1">{tool.command}</span>
          <span class="text-2xs text-text-muted uppercase tracking-[0.5px]">{tool.category}</span>
          {#if tool.isCustom}
            <button
              class="px-2 py-0.5 border-0 rounded-md bg-accent-bg text-accent-text text-xs font-inherit cursor-pointer hover:bg-accent-bg-hover"
              onclick={() => startEdit(tool)}>Edit</button
            >
            <button
              class="px-2 py-0.5 border-0 rounded-md bg-danger-bg text-danger-text text-xs font-inherit cursor-pointer"
              onclick={() => removeTool(tool.id, tool.name)}>Remove</button
            >
          {:else}
            <span class="text-2xs text-text-faint">built-in</span>
          {/if}
        </div>
      {/if}
    {/each}
  </div>

  {#if showForm}
    <!-- svelte-ignore a11y_no_static_element_interactions -->
    <div
      class="flex flex-col gap-2 p-3 border border-border rounded-xl bg-border-subtle"
      onkeydown={(e) => {
        if (e.key === 'Enter') {
          e.preventDefault()
          addTool()
        }
        if (e.key === 'Escape') showForm = false
      }}
    >
      <input
        class="px-2.5 py-1.5 border border-border rounded-lg bg-hover text-text text-md font-inherit outline-none focus:border-focus-ring"
        bind:value={newId}
        placeholder="ID (e.g. my-tool)"
      />
      <input
        class="px-2.5 py-1.5 border border-border rounded-lg bg-hover text-text text-md font-inherit outline-none focus:border-focus-ring"
        bind:value={newName}
        placeholder="Display name"
      />
      <input
        class="px-2.5 py-1.5 border border-border rounded-lg bg-hover text-text text-md font-inherit outline-none focus:border-focus-ring"
        bind:value={newCommand}
        placeholder="Command (binary name)"
      />
      <input
        class="px-2.5 py-1.5 border border-border rounded-lg bg-hover text-text text-md font-inherit outline-none focus:border-focus-ring"
        bind:value={newArgs}
        placeholder="Args (comma-separated)"
      />
      <CustomSelect
        value={newCategory}
        options={[
          { value: 'ai', label: 'AI' },
          { value: 'git', label: 'Git' },
          { value: 'system', label: 'System' },
          { value: 'shell', label: 'Shell' },
        ]}
        maxWidth="100%"
        onchange={(v) => (newCategory = v)}
      />
      {#if error}
        <p class="text-sm text-danger m-0">{error}</p>
      {/if}
      <div class="flex justify-end gap-2">
        <button
          class="px-3.5 py-1.5 rounded-lg text-md font-inherit cursor-pointer border-0 bg-active text-text"
          onclick={() => (showForm = false)}>Cancel</button
        >
        <button
          class="px-3.5 py-1.5 rounded-lg text-md font-inherit cursor-pointer border-0 bg-accent-bg text-accent-text hover:bg-accent-bg-hover"
          onclick={addTool}>Add Tool</button
        >
      </div>
    </div>
  {:else}
    <button
      class="self-start px-3.5 py-1.5 border border-dashed border-text-faint rounded-lg bg-transparent text-text-secondary text-md font-inherit cursor-pointer hover:bg-hover hover:text-text"
      onclick={() => (showForm = true)}>+ Add Custom Tool</button
    >
  {/if}
</div>
