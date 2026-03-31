<script lang="ts">
  import { onMount } from 'svelte'
  import ToolIcon from '../shared/ToolIcon.svelte'
  import CustomSelect from '../shared/CustomSelect.svelte'

  interface ToolDef {
    id: string
    name: string
    command: string
    args: string[]
    icon: string
    category: string
    isCustom: boolean
  }

  let tools: ToolDef[] = $state([])
  let showForm = $state(false)
  let newId = $state('')
  let newName = $state('')
  let newCommand = $state('')
  let newArgs = $state('')
  let newCategory = $state('system')
  let error = $state('')

  onMount(async () => {
    tools = await window.api.listTools()
  })

  async function addTool(): Promise<void> {
    if (!newId.trim() || !newName.trim() || !newCommand.trim()) {
      error = 'ID, name, and command are required'
      return
    }
    if (tools.some((t) => t.id === newId.trim())) {
      error = 'Tool ID already exists'
      return
    }

    try {
      tools = await window.api.addCustomTool({
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
    tools = await window.api.removeCustomTool(id)
  }
</script>

<div class="section">
  <h3 class="section-title">Tools</h3>

  <div class="tool-list">
    {#each tools as tool (tool.id)}
      <div class="tool-row">
        <ToolIcon icon={tool.icon} size={16} />
        <span class="tool-name">{tool.name}</span>
        <span class="tool-command">{tool.command}</span>
        <span class="tool-category">{tool.category}</span>
        {#if tool.isCustom}
          <button class="remove-btn" onclick={() => removeTool(tool.id)}>Remove</button>
        {:else}
          <span class="builtin-badge">built-in</span>
        {/if}
      </div>
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
</style>
