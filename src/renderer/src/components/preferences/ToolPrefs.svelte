<script lang="ts">
  import { Plus, Pencil, Trash2 } from '@lucide/svelte'
  import { getTools } from '../../lib/stores/tools.svelte'
  import { confirm } from '../../lib/stores/dialogs.svelte'
  import ToolIcon from '../shared/ToolIcon.svelte'
  import CustomSelect from '../shared/CustomSelect.svelte'
  import PrefsSection from './_partials/PrefsSection.svelte'
  import { prefsSearch, matches } from './_partials/prefsSearch.svelte'

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

  const categoryOptions = [
    { value: 'ai', label: 'AI' },
    { value: 'git', label: 'Git' },
    { value: 'system', label: 'System' },
    { value: 'shell', label: 'Shell' },
  ]

  function resetNewForm(): void {
    newId = ''
    newName = ''
    newCommand = ''
    newArgs = ''
    newCategory = 'system'
    error = ''
  }

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
      resetNewForm()
      showForm = false
    } catch (e) {
      error = e instanceof Error ? e.message : String(e)
    }
  }

  async function removeTool(id: string, name: string): Promise<void> {
    const ok = await confirm({
      title: 'Remove tool',
      message: `Remove tool "${name}"? This cannot be undone.`,
      confirmLabel: 'Remove',
      destructive: true,
    })
    if (!ok) return
    await window.api.removeCustomTool(id)
  }

  function startEdit(tool: {
    id: string
    name: string
    command: string
    args: string[]
    category: string
  }): void {
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

  function visible(tool: { id: string; name: string; command: string; category: string }): boolean {
    if (prefsSearch.query.trim() === '') return true
    return matches(`${tool.name} ${tool.command} ${tool.category} ${tool.id}`)
  }
</script>

<div class="flex flex-col gap-7">
  <PrefsSection
    title="Tools"
    description="Register custom CLI tools that appear in the command palette and can be opened as tabs"
  >
    <div class="flex flex-col">
      {#each getTools() as tool (tool.id)}
        {#if editingId === tool.id}
          <!-- svelte-ignore a11y_no_static_element_interactions -->
          <div
            class="flex flex-col gap-2 p-3 my-1 border border-border rounded-md bg-bg-input"
            onkeydown={(e) => {
              if (e.key === 'Enter') {
                e.preventDefault()
                saveEdit()
              }
              if (e.key === 'Escape') cancelEdit()
            }}
          >
            <div
              class="flex items-center gap-2 text-2xs uppercase tracking-caps-tight text-text-faint"
            >
              <span>Editing</span>
              <code class="font-mono text-text-muted normal-case tracking-normal">{tool.id}</code>
            </div>
            <input
              class="px-2.5 py-1.5 border border-border rounded-md bg-bg text-text text-md font-inherit outline-none focus:border-focus-ring placeholder:text-text-faint"
              name="editName"
              aria-label="Display name"
              bind:value={editName}
              placeholder="Display name"
              spellcheck="false"
            />
            <input
              class="px-2.5 py-1.5 border border-border rounded-md bg-bg text-text text-md font-mono outline-none focus:border-focus-ring placeholder:text-text-faint"
              name="editCommand"
              aria-label="Command"
              bind:value={editCommand}
              placeholder="Command (binary name)"
              spellcheck="false"
            />
            <input
              class="px-2.5 py-1.5 border border-border rounded-md bg-bg text-text text-md font-mono outline-none focus:border-focus-ring placeholder:text-text-faint"
              name="editArgs"
              aria-label="Args"
              bind:value={editArgs}
              placeholder="Args (comma-separated)"
              spellcheck="false"
            />
            <CustomSelect
              value={editCategory}
              options={categoryOptions}
              maxWidth="100%"
              onchange={(v) => (editCategory = v)}
            />
            {#if editError}
              <p class="text-sm text-danger-text m-0">{editError}</p>
            {/if}
            <div class="flex justify-end gap-2">
              <button
                type="button"
                class="px-3 py-1 rounded-md text-sm font-inherit cursor-pointer border border-border bg-transparent text-text-secondary hover:bg-hover hover:text-text"
                onclick={cancelEdit}>Cancel</button
              >
              <button
                type="button"
                class="px-3 py-1 rounded-md text-sm font-inherit cursor-pointer border-0 bg-accent-bg text-accent-text hover:bg-accent-bg-hover"
                onclick={saveEdit}>Save</button
              >
            </div>
          </div>
        {:else}
          <div
            class="group/tool flex items-center gap-3 py-2 border-t border-border-subtle first:border-t-0 first:pt-0 transition-opacity duration-fast"
            class:opacity-30={!visible(tool)}
          >
            <ToolIcon icon={tool.icon} size={16} />
            <span class="text-md text-text min-w-30 truncate">{tool.name}</span>
            <code class="text-sm text-text-secondary font-mono flex-1 truncate">{tool.command}</code
            >
            <span class="text-2xs uppercase tracking-caps-tight text-text-muted shrink-0"
              >{tool.category}</span
            >
            {#if tool.isCustom}
              <div class="flex items-center gap-0.5 shrink-0">
                <button
                  type="button"
                  class="flex items-center justify-center size-7 rounded-md bg-transparent border-0 text-text-muted cursor-pointer hover:bg-hover hover:text-text"
                  onclick={() => startEdit(tool)}
                  aria-label="Edit {tool.name}"
                  title="Edit"
                >
                  <Pencil size={13} />
                </button>
                <button
                  type="button"
                  class="flex items-center justify-center size-7 rounded-md bg-transparent border-0 text-text-muted cursor-pointer hover:bg-danger-bg hover:text-danger-text"
                  onclick={() => removeTool(tool.id, tool.name)}
                  aria-label="Remove {tool.name}"
                  title="Remove"
                >
                  <Trash2 size={13} />
                </button>
              </div>
            {:else}
              <span class="text-2xs uppercase tracking-caps-tight text-text-faint shrink-0"
                >built-in</span
              >
            {/if}
          </div>
        {/if}
      {/each}
    </div>

    {#if showForm}
      <!-- svelte-ignore a11y_no_static_element_interactions -->
      <div
        class="flex flex-col gap-2 p-3 mt-3 border border-border rounded-md bg-bg-input"
        onkeydown={(e) => {
          if (e.key === 'Enter') {
            e.preventDefault()
            addTool()
          }
          if (e.key === 'Escape') {
            showForm = false
            resetNewForm()
          }
        }}
      >
        <input
          class="px-2.5 py-1.5 border border-border rounded-md bg-bg text-text text-md font-mono outline-none focus:border-focus-ring placeholder:text-text-faint"
          name="newId"
          aria-label="Tool ID"
          bind:value={newId}
          placeholder="ID (e.g. my-tool)"
          spellcheck="false"
        />
        <input
          class="px-2.5 py-1.5 border border-border rounded-md bg-bg text-text text-md font-inherit outline-none focus:border-focus-ring placeholder:text-text-faint"
          name="newName"
          aria-label="Display name"
          bind:value={newName}
          placeholder="Display name"
          spellcheck="false"
        />
        <input
          class="px-2.5 py-1.5 border border-border rounded-md bg-bg text-text text-md font-mono outline-none focus:border-focus-ring placeholder:text-text-faint"
          name="newCommand"
          aria-label="Command"
          bind:value={newCommand}
          placeholder="Command (binary name)"
          spellcheck="false"
        />
        <input
          class="px-2.5 py-1.5 border border-border rounded-md bg-bg text-text text-md font-mono outline-none focus:border-focus-ring placeholder:text-text-faint"
          name="newArgs"
          aria-label="Args"
          bind:value={newArgs}
          placeholder="Args (comma-separated)"
          spellcheck="false"
        />
        <CustomSelect
          value={newCategory}
          options={categoryOptions}
          maxWidth="100%"
          onchange={(v) => (newCategory = v)}
        />
        {#if error}
          <p class="text-sm text-danger-text m-0">{error}</p>
        {/if}
        <div class="flex justify-end gap-2">
          <button
            type="button"
            class="px-3 py-1 rounded-md text-sm font-inherit cursor-pointer border border-border bg-transparent text-text-secondary hover:bg-hover hover:text-text"
            onclick={() => {
              showForm = false
              resetNewForm()
            }}>Cancel</button
          >
          <button
            type="button"
            class="px-3 py-1 rounded-md text-sm font-inherit cursor-pointer border-0 bg-accent-bg text-accent-text hover:bg-accent-bg-hover"
            onclick={addTool}>Add tool</button
          >
        </div>
      </div>
    {:else}
      <button
        type="button"
        class="self-start flex items-center gap-1 px-3 py-1 mt-3 rounded-md bg-border-subtle border border-border text-text-secondary text-sm font-inherit cursor-pointer hover:bg-active hover:text-text"
        onclick={() => (showForm = true)}
      >
        <Plus size={12} />
        <span>Add custom tool</span>
      </button>
    {/if}
  </PrefsSection>
</div>
