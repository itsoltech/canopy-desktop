<script lang="ts">
  import { getTools } from '../../lib/stores/tools.svelte'
  import ToolIcon from '../shared/ToolIcon.svelte'
  import SvgPreview from '../shared/SvgPreview.svelte'
  import CustomSelect from '../shared/CustomSelect.svelte'

  let showForm = $state(false)
  let newId = $state('')
  let newName = $state('')
  let newCommand = $state('')
  let newArgs = $state('')
  let newCategory = $state('system')
  let newIconSvg: string | null = $state(null)
  let error = $state('')

  let editingId: string | null = $state(null)
  let editName = $state('')
  let editCommand = $state('')
  let editArgs = $state('')
  let editCategory = $state('')
  let editIconSvg: string | null = $state(null)
  let editHadIcon = $state(false)
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

    const id = newId.trim()
    try {
      await window.api.addCustomTool({
        id,
        name: newName.trim(),
        command: newCommand.trim(),
        args: newArgs
          .split(',')
          .map((s) => s.trim())
          .filter(Boolean),
        icon: 'terminal',
        category: newCategory,
      })
      if (newIconSvg) {
        await window.api.setToolIcon(id, newIconSvg)
        await window.api.updateCustomTool(id, { icon: `custom:${id}` })
      }
      newId = ''
      newName = ''
      newCommand = ''
      newArgs = ''
      newCategory = 'system'
      newIconSvg = null
      showForm = false
      error = ''
    } catch (e) {
      error = e instanceof Error ? e.message : String(e)
    }
  }

  async function removeTool(id: string): Promise<void> {
    const tool = getTools().find((t) => t.id === id)
    const name = tool?.name ?? id
    if (!confirm(`Remove custom tool "${name}"? This cannot be undone.`)) return
    await window.api.removeCustomTool(id)
  }

  async function startEdit(tool: {
    id: string
    name: string
    command: string
    args: string[]
    icon: string
    category: string
  }): Promise<void> {
    editingId = tool.id
    editName = tool.name
    editCommand = tool.command
    editArgs = tool.args.join(', ')
    editCategory = tool.category
    editHadIcon = tool.icon.startsWith('custom:')
    editError = ''
    if (editHadIcon) {
      editIconSvg = await window.api.getToolIcon(tool.id)
    } else {
      editIconSvg = null
    }
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
      const changes: {
        name?: string
        command?: string
        args?: string[]
        icon?: string
        category?: string
      } = {
        name: editName.trim(),
        command: editCommand.trim(),
        args: editArgs
          .split(',')
          .map((s) => s.trim())
          .filter(Boolean),
        category: editCategory,
      }
      if (editIconSvg) {
        changes.icon = `custom:${editingId}`
      } else if (editHadIcon) {
        changes.icon = 'terminal'
      }
      await window.api.updateCustomTool(editingId, changes)
      if (editIconSvg) {
        await window.api.setToolIcon(editingId, editIconSvg)
      } else if (editHadIcon) {
        await window.api.removeToolIcon(editingId)
      }
      editingId = null
      editIconSvg = null
      editHadIcon = false
      editError = ''
    } catch (e) {
      editError = e instanceof Error ? e.message : String(e)
    }
  }

  async function pickIcon(target: 'new' | 'edit'): Promise<void> {
    const svgContent = await window.api.selectIconFile()
    if (svgContent != null) {
      if (target === 'new') {
        newIconSvg = svgContent
      } else {
        editIconSvg = svgContent
      }
    }
  }
</script>

<div class="section">
  <h3 class="section-title">Tools</h3>

  <div class="tool-list">
    {#each getTools() as tool (tool.id)}
      {#if editingId === tool.id}
        <div class="edit-form">
          <div class="edit-form-header">
            <span class="edit-id-label">ID: {tool.id}</span>
          </div>
          <input class="form-input" bind:value={editName} placeholder="Display name" />
          <input class="form-input" bind:value={editCommand} placeholder="Command (binary name)" />
          <input class="form-input" bind:value={editArgs} placeholder="Args (comma-separated)" />
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
          {#if editIconSvg}
            <div class="icon-preview-row">
              <SvgPreview svg={editIconSvg} size={20} />
              <span class="icon-preview-label">Selected icon</span>
              <button class="btn btn-icon-clear" onclick={() => (editIconSvg = null)}>Clear</button>
            </div>
          {:else}
            <button class="btn btn-icon-upload" onclick={() => pickIcon('edit')}>
              Choose Icon (SVG)
            </button>
          {/if}
          {#if editError}
            <p class="form-error">{editError}</p>
          {/if}
          <div class="form-actions">
            <button class="btn btn-cancel" onclick={cancelEdit}>Cancel</button>
            <button class="btn btn-add" onclick={saveEdit}>Save</button>
          </div>
        </div>
      {:else}
        <div class="tool-row">
          <ToolIcon icon={tool.icon} size={16} />
          <span class="tool-name">{tool.name}</span>
          <span class="tool-command">{tool.command}</span>
          <span class="tool-category">{tool.category}</span>
          {#if tool.isCustom}
            <button class="edit-btn" onclick={() => startEdit(tool)}>Edit</button>
            <button class="remove-btn" onclick={() => removeTool(tool.id)}>Remove</button>
          {:else}
            <span class="builtin-badge">built-in</span>
          {/if}
        </div>
      {/if}
    {/each}
  </div>

  {#if showForm}
    <div class="add-form">
      <input class="form-input" bind:value={newId} placeholder="ID (e.g. my-tool)" />
      <input class="form-input" bind:value={newName} placeholder="Display name" />
      <input class="form-input" bind:value={newCommand} placeholder="Command (binary name)" />
      <input class="form-input" bind:value={newArgs} placeholder="Args (comma-separated)" />
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
      {#if newIconSvg}
        <div class="icon-preview-row">
          <SvgPreview svg={newIconSvg} size={20} />
          <span class="icon-preview-label">Selected icon</span>
          <button class="btn btn-icon-clear" onclick={() => (newIconSvg = null)}>Clear</button>
        </div>
      {:else}
        <button class="btn btn-icon-upload" onclick={() => pickIcon('new')}>
          Choose Icon (SVG)
        </button>
      {/if}
      {#if error}
        <p class="form-error">{error}</p>
      {/if}
      <div class="form-actions">
        <button class="btn btn-cancel" onclick={() => (showForm = false)}>Cancel</button>
        <button class="btn btn-add" onclick={addTool}>Add Tool</button>
      </div>
    </div>
  {:else}
    <button class="btn btn-add-tool" onclick={() => (showForm = true)}>+ Add Custom Tool</button>
  {/if}
</div>

<style>
  .section {
    display: flex;
    flex-direction: column;
    gap: 16px;
  }

  .section-title {
    font-size: 15px;
    font-weight: 600;
    color: #e0e0e0;
    margin: 0;
  }

  .tool-list {
    display: flex;
    flex-direction: column;
    gap: 4px;
  }

  .tool-row {
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 6px 10px;
    border-radius: 6px;
    background: rgba(255, 255, 255, 0.03);
    font-size: 13px;
  }

  .tool-name {
    color: #e0e0e0;
    min-width: 100px;
  }

  .tool-command {
    color: rgba(255, 255, 255, 0.5);
    font-family: monospace;
    font-size: 12px;
    flex: 1;
  }

  .tool-category {
    font-size: 10px;
    color: rgba(255, 255, 255, 0.35);
    text-transform: uppercase;
    letter-spacing: 0.5px;
  }

  .builtin-badge {
    font-size: 10px;
    color: rgba(255, 255, 255, 0.25);
  }

  .remove-btn {
    padding: 2px 8px;
    border: none;
    border-radius: 4px;
    background: rgba(255, 100, 100, 0.15);
    color: rgba(255, 120, 120, 0.8);
    font-size: 11px;
    font-family: inherit;
    cursor: pointer;
  }

  .remove-btn:hover {
    background: rgba(255, 100, 100, 0.25);
  }

  .add-form {
    display: flex;
    flex-direction: column;
    gap: 8px;
    padding: 12px;
    border: 1px solid rgba(255, 255, 255, 0.1);
    border-radius: 8px;
    background: rgba(255, 255, 255, 0.03);
  }

  .form-input {
    padding: 6px 10px;
    border: 1px solid rgba(255, 255, 255, 0.12);
    border-radius: 6px;
    background: rgba(255, 255, 255, 0.06);
    color: #e0e0e0;
    font-size: 13px;
    font-family: inherit;
    outline: none;
  }

  .form-input:focus {
    border-color: rgba(116, 192, 252, 0.5);
  }

  .form-error {
    font-size: 12px;
    color: #ff6b6b;
    margin: 0;
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
    background: rgba(255, 255, 255, 0.08);
    color: rgba(255, 255, 255, 0.7);
  }

  .btn-add {
    background: rgba(116, 192, 252, 0.2);
    color: rgba(116, 192, 252, 0.9);
  }

  .btn-add:hover {
    background: rgba(116, 192, 252, 0.3);
  }

  .btn-add-tool {
    align-self: flex-start;
    padding: 6px 14px;
    border: 1px dashed rgba(255, 255, 255, 0.15);
    border-radius: 6px;
    background: transparent;
    color: rgba(255, 255, 255, 0.5);
    font-size: 13px;
    font-family: inherit;
    cursor: pointer;
  }

  .btn-add-tool:hover {
    background: rgba(255, 255, 255, 0.05);
    color: rgba(255, 255, 255, 0.7);
  }

  .edit-form {
    display: flex;
    flex-direction: column;
    gap: 8px;
    padding: 12px;
    border: 1px solid rgba(116, 192, 252, 0.2);
    border-radius: 8px;
    background: rgba(255, 255, 255, 0.03);
    width: 100%;
  }

  .edit-form-header {
    margin-bottom: 4px;
  }

  .edit-id-label {
    font-size: 11px;
    color: rgba(255, 255, 255, 0.4);
    font-family: monospace;
  }

  .edit-btn {
    padding: 2px 8px;
    border: none;
    border-radius: 4px;
    background: rgba(116, 192, 252, 0.15);
    color: rgba(116, 192, 252, 0.8);
    font-size: 11px;
    font-family: inherit;
    cursor: pointer;
  }

  .edit-btn:hover {
    background: rgba(116, 192, 252, 0.25);
  }

  .btn-icon-upload {
    background: rgba(255, 255, 255, 0.06);
    color: rgba(255, 255, 255, 0.5);
    border: 1px dashed rgba(255, 255, 255, 0.15);
    font-size: 12px;
  }

  .btn-icon-upload:hover {
    background: rgba(255, 255, 255, 0.1);
    color: rgba(255, 255, 255, 0.7);
  }

  .icon-preview-row {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 6px 10px;
    border: 1px solid rgba(255, 255, 255, 0.1);
    border-radius: 6px;
    background: rgba(255, 255, 255, 0.03);
  }

  .icon-preview-label {
    font-size: 12px;
    color: rgba(255, 255, 255, 0.5);
    flex: 1;
  }

  .btn-icon-clear {
    padding: 2px 8px;
    border: none;
    border-radius: 4px;
    background: rgba(255, 100, 100, 0.15);
    color: rgba(255, 120, 120, 0.8);
    font-size: 11px;
    font-family: inherit;
    cursor: pointer;
  }

  .btn-icon-clear:hover {
    background: rgba(255, 100, 100, 0.25);
  }
</style>
