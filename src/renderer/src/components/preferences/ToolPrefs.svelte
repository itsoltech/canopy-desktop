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
</style>
