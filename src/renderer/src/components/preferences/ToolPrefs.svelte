<script lang="ts">
  import { tick } from 'svelte'
  import { Plus, Pencil, Trash2, Eye, EyeOff, ChevronUp, ChevronDown } from '@lucide/svelte'
  import { getTools } from '../../lib/stores/tools.svelte'
  import {
    getToolView,
    toggleToolVisibility,
    moveToolUp,
    moveToolDown,
    removeToolFromView,
  } from '../../lib/stores/toolView.svelte'
  import { confirm } from '../../lib/stores/dialogs.svelte'
  import ToolIcon from '../shared/ToolIcon.svelte'
  import PrefsSection from './_partials/PrefsSection.svelte'
  import ToolForm from './_partials/ToolForm.svelte'
  import { prefsSearch, matches } from './_partials/prefsSearch.svelte'

  let toolsById = $derived(new Map(getTools().map((t) => [t.id, t])))
  let view = $derived(getToolView())

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

  // Removal happens outside any form, so it has no inline error slot like
  // addTool/saveEdit. Surface IPC failures in a dedicated banner instead of
  // letting the rejection go unhandled and the tool silently persist.
  let removeError = $state('')

  // Reorder moves focus to the arrow at the new index, but the button's
  // accessible name is static, so a screen-reader user gets no signal that the
  // order changed. Announce the new position through a visually-hidden live
  // region after each successful move.
  let reorderStatus = $state('')

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
    try {
      await window.api.removeCustomTool(id)
      removeToolFromView(id)
      removeError = ''
    } catch (e) {
      removeError = `Couldn't remove "${name}": ${e instanceof Error ? e.message : String(e)}`
    }
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

  function matchesSearch(tool: {
    id: string
    name: string
    command: string
    category: string
  }): boolean {
    if (prefsSearch.query.trim() === '') return true
    return matches(`${tool.name} ${tool.command} ${tool.category} ${tool.id}`)
  }

  // Keep keyboard focus on the moved row. Focus the order button matching the
  // move direction at the new index; if that button is disabled (the row hit
  // the top/bottom boundary, where focus() is a no-op and would drop to
  // <body>), fall back to the opposite-direction button on the same row.
  // querySelector returns Element | null, so we narrow with instanceof rather
  // than an unchecked cast.
  async function focusOrderButton(direction: 'up' | 'down', index: number): Promise<void> {
    await tick()
    const primary = document.querySelector(`[data-tool-order-${direction}="${index}"]`)
    if (primary instanceof HTMLButtonElement && !primary.disabled) {
      primary.focus()
      return
    }
    const opposite = direction === 'up' ? 'down' : 'up'
    const fallback = document.querySelector(`[data-tool-order-${opposite}="${index}"]`)
    if (fallback instanceof HTMLButtonElement) fallback.focus()
  }

  function announceReorder(id: string, newIndex: number): void {
    const name = toolsById.get(id)?.name ?? id
    reorderStatus = `${name} moved to position ${newIndex + 1} of ${view.length}`
  }

  async function onMoveUp(id: string, index: number): Promise<void> {
    if (index === 0) return
    moveToolUp(id)
    announceReorder(id, index - 1)
    await focusOrderButton('up', index - 1)
  }

  async function onMoveDown(id: string, index: number): Promise<void> {
    if (index === view.length - 1) return
    moveToolDown(id)
    announceReorder(id, index + 1)
    await focusOrderButton('down', index + 1)
  }
</script>

<div class="flex flex-col gap-7">
  <PrefsSection
    title="Tools"
    description="Register custom CLI tools, reorder them, and choose which appear in the sidebar. Hidden tools stay searchable in the command palette."
  >
    {#if removeError}
      <p class="text-sm text-danger-text mb-2" role="alert">{removeError}</p>
    {/if}
    <span class="sr-only" role="status" aria-live="polite">{reorderStatus}</span>
    <div class="flex flex-col">
      {#each view as entry, i (entry.id)}
        {@const tool = toolsById.get(entry.id)}
        {#if tool}
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
              class:opacity-30={!matchesSearch(tool)}
            >
              <div class="flex items-center gap-3 flex-1 min-w-0">
                <ToolIcon icon={tool.icon} size={16} />
                <span class="text-md text-text min-w-30 truncate" title={tool.name}
                  >{tool.name}</span
                >
                <code
                  class="text-sm text-text-secondary font-mono flex-1 truncate"
                  title={tool.command}>{tool.command}</code
                >
                {#if !entry.visible}
                  <span
                    class="inline-flex items-center gap-1 shrink-0 text-2xs uppercase tracking-caps-tight text-text-muted bg-border-subtle rounded-sm px-1.5 py-0.5"
                    title="Hidden from sidebar"
                  >
                    <EyeOff size={11} />
                    Hidden
                  </span>
                {/if}
              </div>
              <span class="text-2xs uppercase tracking-caps-tight text-text-muted shrink-0"
                >{tool.category}</span
              >
              <div class="flex items-center gap-0.5 shrink-0">
                {#if tool.isCustom}
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
                {:else}
                  <span class="text-2xs uppercase tracking-caps-tight text-text-faint mr-1"
                    >built-in</span
                  >
                {/if}
                <button
                  type="button"
                  class="flex items-center justify-center size-7 rounded-md bg-transparent border-0 text-text-muted cursor-pointer hover:bg-hover hover:text-text"
                  onclick={() => toggleToolVisibility(tool.id)}
                  aria-label={entry.visible
                    ? `Hide ${tool.name} from sidebar`
                    : `Show ${tool.name} in sidebar`}
                  title={entry.visible ? 'Hide from sidebar' : 'Show in sidebar'}
                >
                  {#if entry.visible}
                    <Eye size={13} />
                  {:else}
                    <EyeOff size={13} />
                  {/if}
                </button>
                <button
                  type="button"
                  class="flex items-center justify-center size-7 rounded-md bg-transparent border-0 text-text-muted cursor-pointer enabled:hover:bg-hover enabled:hover:text-text disabled:opacity-20 disabled:cursor-default"
                  data-tool-order-up={i}
                  disabled={i === 0}
                  onclick={() => onMoveUp(tool.id, i)}
                  aria-label="Move {tool.name} up"
                  title="Move up"
                >
                  <ChevronUp size={14} />
                </button>
                <button
                  type="button"
                  class="flex items-center justify-center size-7 rounded-md bg-transparent border-0 text-text-muted cursor-pointer enabled:hover:bg-hover enabled:hover:text-text disabled:opacity-20 disabled:cursor-default"
                  data-tool-order-down={i}
                  disabled={i === view.length - 1}
                  onclick={() => onMoveDown(tool.id, i)}
                  aria-label="Move {tool.name} down"
                  title="Move down"
                >
                  <ChevronDown size={14} />
                </button>
              </div>
            </div>
          {/if}
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
