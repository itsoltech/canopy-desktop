<script lang="ts">
  import { Plus, Pencil, Trash2 } from '@lucide/svelte'
  import { getTools } from '../../lib/stores/tools.svelte'
  import { confirm } from '../../lib/stores/dialogs.svelte'
  import ToolIcon from '../shared/ToolIcon.svelte'
  import PrefsSection from './_partials/PrefsSection.svelte'
  import ToolForm from './_partials/ToolForm.svelte'
  import { prefsSearch, matches } from './_partials/prefsSearch.svelte'

  interface ToolDraft {
    id: string
    name: string
    command: string
    args: string
    category: string
  }

  function emptyDraft(): ToolDraft {
    return { id: '', name: '', command: '', args: '', category: 'system' }
  }

  let showForm = $state(false)
  let newDraft = $state<ToolDraft>(emptyDraft())
  let error = $state('')

  let editingId: string | null = $state(null)
  let editDraft = $state<ToolDraft>(emptyDraft())
  let editError = $state('')

  function parseArgs(s: string): string[] {
    return s
      .split(',')
      .map((x) => x.trim())
      .filter(Boolean)
  }

  async function addTool(): Promise<void> {
    if (!newDraft.id.trim() || !newDraft.name.trim() || !newDraft.command.trim()) {
      error = 'ID, name, and command are required'
      return
    }
    if (getTools().some((t) => t.id === newDraft.id.trim())) {
      error = 'Tool ID already exists'
      return
    }

    try {
      await window.api.addCustomTool({
        id: newDraft.id.trim(),
        name: newDraft.name.trim(),
        command: newDraft.command.trim(),
        args: parseArgs(newDraft.args),
        category: newDraft.category,
      })
      newDraft = emptyDraft()
      error = ''
      showForm = false
    } catch (e) {
      error = e instanceof Error ? e.message : String(e)
    }
  }

  function cancelAdd(): void {
    newDraft = emptyDraft()
    error = ''
    showForm = false
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
    editDraft = {
      id: tool.id,
      name: tool.name,
      command: tool.command,
      args: tool.args.join(', '),
      category: tool.category,
    }
    editError = ''
  }

  function cancelEdit(): void {
    editingId = null
    editError = ''
  }

  async function saveEdit(): Promise<void> {
    if (!editingId) return
    if (!editDraft.name.trim() || !editDraft.command.trim()) {
      editError = 'Name and command are required'
      return
    }

    try {
      await window.api.updateCustomTool(editingId, {
        name: editDraft.name.trim(),
        command: editDraft.command.trim(),
        args: parseArgs(editDraft.args),
        category: editDraft.category,
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
          <ToolForm
            bind:draft={editDraft}
            mode="edit"
            error={editError}
            onCancel={cancelEdit}
            onSubmit={saveEdit}
          />
        {:else}
          <div
            class="group/tool flex items-center gap-3 py-2 border-t border-border-subtle first:border-t-0 first:pt-0 transition-opacity duration-fast"
            class:opacity-30={!visible(tool)}
          >
            <ToolIcon icon={tool.icon} size={16} />
            <span class="text-md text-text min-w-30 truncate" title={tool.name}>{tool.name}</span>
            <code class="text-sm text-text-secondary font-mono flex-1 truncate" title={tool.command}
              >{tool.command}</code
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
      <ToolForm bind:draft={newDraft} mode="add" {error} onCancel={cancelAdd} onSubmit={addTool} />
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
