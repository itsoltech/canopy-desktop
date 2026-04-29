<script lang="ts">
  import CustomCheckbox from '../../shared/CustomCheckbox.svelte'
  import CustomRadio from '../../shared/CustomRadio.svelte'

  let {
    agentLabels,
    workspacePath,
    onClose,
    onInstalled,
  }: {
    agentLabels: Record<string, string>
    workspacePath?: string
    onClose: () => void
    onInstalled: () => void | Promise<void>
  } = $props()

  let source = $state('')
  let scope = $state('project')
  let method = $state('copy')
  let agents = $state<string[]>(['claude'])
  let error = $state('')
  let installing = $state(false)

  function reset(): void {
    source = ''
    agents = ['claude']
    scope = 'project'
    method = 'copy'
    error = ''
  }

  function toggleAgent(agent: string): void {
    if (agents.includes(agent)) {
      agents = agents.filter((a) => a !== agent)
    } else {
      agents = [...agents, agent]
    }
  }

  async function install(): Promise<void> {
    if (!source.trim()) {
      error = 'Source is required'
      return
    }

    installing = true
    error = ''

    try {
      await window.api.installSkill({
        source: source.trim(),
        agents: [...agents],
        scope: String(scope),
        method: String(method),
        workspacePath,
      })
      reset()
      await onInstalled()
      onClose()
    } catch (e) {
      error = e instanceof Error ? e.message : String(e)
    } finally {
      installing = false
    }
  }

  function cancel(): void {
    reset()
    onClose()
  }
</script>

<form
  class="flex flex-col gap-2 p-3 border border-border rounded-md bg-bg-input"
  onsubmit={(e) => {
    e.preventDefault()
    install()
  }}
  onkeydown={(e) => {
    if (e.key === 'Escape') cancel()
  }}
>
  <input
    class="px-2.5 py-1.5 border border-border rounded-md bg-bg text-text text-md font-inherit outline-none focus:border-focus-ring placeholder:text-text-faint"
    name="installSource"
    aria-label="Skill source"
    bind:value={source}
    placeholder="github:owner/repo/path, local path, or URL"
    spellcheck="false"
    autocomplete="off"
  />

  <div class="flex flex-col gap-1.5">
    <span class="text-2xs font-semibold uppercase tracking-caps-tight text-text-faint">Agents</span>
    <div class="flex gap-3 flex-wrap">
      {#each Object.entries(agentLabels) as [key, label] (key)}
        <label class="flex items-center gap-1.5 text-md text-text cursor-pointer">
          <CustomCheckbox checked={agents.includes(key)} onchange={() => toggleAgent(key)} />
          <span>{label}</span>
        </label>
      {/each}
    </div>
  </div>

  <div class="flex flex-col gap-1.5">
    <span class="text-2xs font-semibold uppercase tracking-caps-tight text-text-faint">Scope</span>
    <div class="flex gap-3 flex-wrap">
      <label class="flex items-center gap-1.5 text-md text-text cursor-pointer">
        <CustomRadio checked={scope === 'project'} onchange={() => (scope = 'project')} />
        <span>Project</span>
      </label>
      <label class="flex items-center gap-1.5 text-md text-text cursor-pointer">
        <CustomRadio checked={scope === 'global'} onchange={() => (scope = 'global')} />
        <span>Global</span>
      </label>
    </div>
  </div>

  <div class="flex flex-col gap-1.5">
    <span class="text-2xs font-semibold uppercase tracking-caps-tight text-text-faint">Method</span>
    <div class="flex gap-3 flex-wrap">
      <label class="flex items-center gap-1.5 text-md text-text cursor-pointer">
        <CustomRadio checked={method === 'copy'} onchange={() => (method = 'copy')} />
        <span>Copy</span>
      </label>
      <label class="flex items-center gap-1.5 text-md text-text cursor-pointer">
        <CustomRadio checked={method === 'symlink'} onchange={() => (method = 'symlink')} />
        <span>Symlink</span>
      </label>
    </div>
  </div>

  {#if error}
    <p class="text-sm text-danger-text m-0">{error}</p>
  {/if}

  <div class="flex justify-end gap-2">
    <button
      type="button"
      class="px-3 py-1 rounded-md text-sm font-inherit cursor-pointer border border-border bg-transparent text-text-secondary hover:bg-hover hover:text-text"
      onclick={cancel}>Cancel</button
    >
    <button
      type="submit"
      class="px-3 py-1 rounded-md text-sm font-inherit cursor-pointer border-0 bg-accent-bg text-accent-text disabled:opacity-60 disabled:cursor-default hover:bg-accent-bg-hover"
      disabled={installing}
    >
      {installing ? 'Installing…' : 'Install'}
    </button>
  </div>
</form>
