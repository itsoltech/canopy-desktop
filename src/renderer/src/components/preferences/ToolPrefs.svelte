<script lang="ts">
  import { getTools } from '../../lib/stores/tools.svelte'
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

  async function removeTool(id: string): Promise<void> {
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

<div class="section">
  <h3 class="section-title">Tools</h3>

  <div class="tool-list">
    {#each getTools() as tool (tool.id)}
      {#if editingId === tool.id}
        <!-- svelte-ignore a11y_no_static_element_interactions -->
        <div
          class="edit-form"
          onkeydown={(e) => {
            if (e.key === 'Enter') {
              e.preventDefault()
              saveEdit()
            }
            if (e.key === 'Escape') cancelEdit()
          }}
        >
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
    <!-- svelte-ignore a11y_no_static_element_interactions -->
    <div
      class="add-form"
      onkeydown={(e) => {
        if (e.key === 'Enter') {
          e.preventDefault()
          addTool()
        }
        if (e.key === 'Escape') showForm = false
      }}
    >
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
    color: var(--c-text);
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
    background: var(--c-border-subtle);
    font-size: 13px;
  }

  .tool-name {
    color: var(--c-text);
    min-width: 100px;
  }

  .tool-command {
    color: var(--c-text-secondary);
    font-family: monospace;
    font-size: 12px;
    flex: 1;
  }

  .tool-category {
    font-size: 10px;
    color: var(--c-text-muted);
    text-transform: uppercase;
    letter-spacing: 0.5px;
  }

  .builtin-badge {
    font-size: 10px;
    color: var(--c-text-faint);
  }

  .remove-btn {
    padding: 2px 8px;
    border: none;
    border-radius: 4px;
    background: var(--c-danger-bg);
    color: var(--c-danger-text);
    font-size: 11px;
    font-family: inherit;
    cursor: pointer;
  }

  .remove-btn:hover {
    background: var(--c-danger-bg);
  }

  .add-form {
    display: flex;
    flex-direction: column;
    gap: 8px;
    padding: 12px;
    border: 1px solid var(--c-border);
    border-radius: 8px;
    background: var(--c-border-subtle);
  }

  .form-input {
    padding: 6px 10px;
    border: 1px solid var(--c-border);
    border-radius: 6px;
    background: var(--c-hover);
    color: var(--c-text);
    font-size: 13px;
    font-family: inherit;
    outline: none;
  }

  .form-input:focus {
    border-color: var(--c-focus-ring);
  }

  .form-error {
    font-size: 12px;
    color: var(--c-danger);
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
    background: var(--c-active);
    color: var(--c-text);
  }

  .btn-add {
    background: var(--c-accent-bg);
    color: var(--c-accent-text);
  }

  .btn-add:hover {
    background: var(--c-accent-bg-hover);
  }

  .btn-add-tool {
    align-self: flex-start;
    padding: 6px 14px;
    border: 1px dashed var(--c-text-faint);
    border-radius: 6px;
    background: transparent;
    color: var(--c-text-secondary);
    font-size: 13px;
    font-family: inherit;
    cursor: pointer;
  }

  .btn-add-tool:hover {
    background: var(--c-hover);
    color: var(--c-text);
  }

  .edit-form {
    display: flex;
    flex-direction: column;
    gap: 8px;
    padding: 12px;
    border: 1px solid var(--c-border);
    border-radius: 8px;
    background: var(--c-border-subtle);
    width: 100%;
  }

  .edit-form-header {
    margin-bottom: 4px;
  }

  .edit-id-label {
    font-size: 11px;
    color: var(--c-text-faint);
    font-family: monospace;
  }

  .edit-btn {
    padding: 2px 8px;
    border: none;
    border-radius: 4px;
    background: var(--c-accent-bg);
    color: var(--c-accent-text);
    font-size: 11px;
    font-family: inherit;
    cursor: pointer;
  }

  .edit-btn:hover {
    background: var(--c-accent-bg-hover);
  }
</style>
