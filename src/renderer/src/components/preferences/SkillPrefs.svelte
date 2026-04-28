<script lang="ts">
  import { onMount } from 'svelte'
  import { getSkills } from '../../lib/stores/skills.svelte'
  import { confirm } from '../../lib/stores/dialogs.svelte'
  import { workspaceState } from '../../lib/stores/workspace.svelte'

  const skills = $derived(getSkills())

  onMount(() => {
    scanForSkills()
  })

  const agentLabels: Record<string, string> = {
    claude: 'Claude',
    gemini: 'Gemini',
    cursor: 'Cursor',
    opencode: 'OpenCode',
  }

  let showInstallForm = $state(false)
  let installSource = $state('')
  let installScope = $state('project')
  let installMethod = $state('copy')
  let installAgents = $state<string[]>(['claude'])
  let installError = $state('')
  let installing = $state(false)

  let expandedId: string | null = $state(null)

  let scannedSkills = $state<
    Array<{
      id: string
      name: string
      description: string
      agent: string
      scope: string
      filePath: string
    }>
  >([])
  let scanning = $state(false)

  const projectScanned = $derived(scannedSkills.filter((s) => s.scope === 'project'))
  const globalScanned = $derived(scannedSkills.filter((s) => s.scope === 'global'))

  async function scanForSkills(): Promise<void> {
    scanning = true
    try {
      scannedSkills = await window.api.scanSkills(workspaceState.selectedWorktreePath ?? undefined)
    } catch (e) {
      console.error('Scan failed:', e)
    } finally {
      scanning = false
    }
  }

  async function installSkill(): Promise<void> {
    if (!installSource.trim()) {
      installError = 'Source is required'
      return
    }

    installing = true
    installError = ''

    try {
      await window.api.installSkill({
        source: installSource.trim(),
        agents: [...installAgents],
        scope: String(installScope),
        method: String(installMethod),
        workspacePath: workspaceState.selectedWorktreePath ?? undefined,
      })
      installSource = ''
      installAgents = ['claude']
      installScope = 'project'
      installMethod = 'copy'
      showInstallForm = false
      await scanForSkills()
    } catch (e) {
      installError = e instanceof Error ? e.message : String(e)
    } finally {
      installing = false
    }
  }

  async function removeSkill(id: string, name: string): Promise<void> {
    const ok = await confirm({
      title: 'Remove Skill',
      message: `Remove skill "${name}"?`,
      confirmLabel: 'Remove',
      destructive: true,
    })
    if (!ok) return
    try {
      await window.api.removeSkill(id, workspaceState.selectedWorktreePath ?? undefined)
    } catch (e) {
      console.error('Failed to remove skill:', e)
    }
  }

  async function deleteScannedSkill(filePath: string, name: string): Promise<void> {
    const ok = await confirm({
      title: 'Delete Skill File',
      message: `Delete skill file "${name}" from disk? This cannot be undone.`,
      confirmLabel: 'Delete',
      destructive: true,
    })
    if (!ok) return
    try {
      await window.api.deleteSkillFile(filePath)
      await scanForSkills()
    } catch (e) {
      console.error('Failed to delete skill file:', e)
    }
  }

  async function updateSkill(id: string): Promise<void> {
    try {
      await window.api.updateSkill(id, workspaceState.selectedWorktreePath ?? undefined)
    } catch (e) {
      console.error('Failed to update skill:', e)
    }
  }

  async function toggleAgent(skillId: string, agent: string, enabled: boolean): Promise<void> {
    try {
      await window.api.toggleSkillAgent(
        skillId,
        agent,
        enabled,
        workspaceState.selectedWorktreePath ?? undefined,
      )
    } catch (e) {
      console.error('Failed to toggle agent:', e)
    }
  }

  function toggleExpand(id: string): void {
    expandedId = expandedId === id ? null : id
  }

  function toggleInstallAgent(agent: string): void {
    if (installAgents.includes(agent)) {
      installAgents = installAgents.filter((a) => a !== agent)
    } else {
      installAgents = [...installAgents, agent]
    }
  }
</script>

{#snippet skillRow(
  id: string,
  name: string,
  agentDisplay: string,
  details: import('svelte').Snippet,
)}
  <div class="rounded-lg bg-border-subtle overflow-hidden">
    <div
      class="flex items-center gap-2.5 px-2.5 py-1.5 text-md cursor-pointer hover:bg-hover"
      role="button"
      tabindex="0"
      onclick={() => toggleExpand(id)}
      onkeydown={(e) => {
        if (e.key === 'Enter' || e.key === ' ') {
          e.preventDefault()
          toggleExpand(id)
        }
      }}
    >
      <span class="text-text min-w-[120px] font-medium">{name}</span>
      <span class="text-text-secondary text-sm flex-1">{agentDisplay}</span>
    </div>
    {#if expandedId === id}
      <div class="px-3 py-2.5 border-t border-border flex flex-col gap-2.5">
        {@render details()}
      </div>
    {/if}
  </div>
{/snippet}

<div class="flex flex-col gap-4">
  <h3 class="text-[15px] font-semibold text-text m-0">Skills</h3>

  {#if skills.length > 0}
    <div class="flex flex-col gap-1">
      {#each skills as skill (skill.id)}
        {@const agentDisplay = skill.enabledAgents.map((a) => agentLabels[a] ?? a).join(', ')}
        {#snippet details()}
          <p class="text-sm text-text-secondary m-0 leading-snug">
            {skill.description || 'No description'}
          </p>
          <div class="flex flex-col gap-0.5">
            <span class="text-xs font-semibold text-text-secondary uppercase tracking-[0.3px]"
              >Source</span
            >
            <span class="text-sm text-text break-all">{skill.sourceUri}</span>
          </div>
          <div class="flex gap-6">
            <div class="flex flex-col gap-0.5">
              <span class="text-xs font-semibold text-text-secondary uppercase tracking-[0.3px]"
                >Scope</span
              >
              <span class="text-sm text-text">{skill.scope}</span>
            </div>
            <div class="flex flex-col gap-0.5">
              <span class="text-xs font-semibold text-text-secondary uppercase tracking-[0.3px]"
                >Type</span
              >
              <span class="text-sm text-text">{skill.sourceType}</span>
            </div>
            <div class="flex flex-col gap-0.5">
              <span class="text-xs font-semibold text-text-secondary uppercase tracking-[0.3px]"
                >Method</span
              >
              <span class="text-sm text-text">{skill.installMethod}</span>
            </div>
            <div class="flex flex-col gap-0.5">
              <span class="text-xs font-semibold text-text-secondary uppercase tracking-[0.3px]"
                >Version</span
              >
              <span class="text-sm text-text">{skill.version}</span>
            </div>
          </div>
          <div class="flex flex-col gap-1">
            <span class="text-xs font-semibold text-text-secondary uppercase tracking-[0.3px]"
              >Agents</span
            >
            <div class="flex gap-3.5">
              {#each skill.agents as agent (agent)}
                <label class="flex items-center gap-1.5 text-md text-text cursor-pointer">
                  <input
                    type="checkbox"
                    checked={skill.enabledAgents.includes(agent)}
                    onchange={() =>
                      toggleAgent(skill.id, agent, !skill.enabledAgents.includes(agent))}
                  />
                  {agentLabels[agent] ?? agent}
                </label>
              {/each}
            </div>
          </div>
          <div class="flex gap-2 pt-1">
            <button
              class="px-3.5 py-1 rounded-lg text-sm font-inherit cursor-pointer border-0 bg-accent-bg text-accent-text hover:bg-accent-bg-hover"
              onclick={() => updateSkill(skill.id)}>Update</button
            >
            <button
              class="px-3.5 py-1 rounded-lg text-sm font-inherit cursor-pointer border-0 bg-danger-bg text-danger-text"
              onclick={() => removeSkill(skill.id, skill.name)}>Remove</button
            >
          </div>
        {/snippet}
        {@render skillRow(skill.id, skill.name, agentDisplay, details)}
      {/each}
    </div>
  {/if}

  {#if scannedSkills.length > 0}
    {#if projectScanned.length > 0}
      <span class="text-xs font-semibold text-text-secondary uppercase tracking-[0.4px]"
        >Project Skills</span
      >
      <div class="flex flex-col gap-1">
        {#each projectScanned as skill (skill.filePath)}
          {#snippet details()}
            <p class="text-sm text-text-secondary m-0 leading-snug">
              {skill.description || 'No description'}
            </p>
            <div class="flex gap-6">
              <div class="flex flex-col gap-0.5">
                <span class="text-xs font-semibold text-text-secondary uppercase tracking-[0.3px]"
                  >Agent</span
                >
                <span class="text-sm text-text">{agentLabels[skill.agent] ?? skill.agent}</span>
              </div>
              <div class="flex flex-col gap-0.5">
                <span class="text-xs font-semibold text-text-secondary uppercase tracking-[0.3px]"
                  >Scope</span
                >
                <span class="text-sm text-text">{skill.scope}</span>
              </div>
            </div>
            <div class="flex flex-col gap-0.5">
              <span class="text-xs font-semibold text-text-secondary uppercase tracking-[0.3px]"
                >File</span
              >
              <span class="text-sm text-text break-all">{skill.filePath}</span>
            </div>
            <div class="flex gap-2 pt-1">
              <button
                class="px-3.5 py-1 rounded-lg text-sm font-inherit cursor-pointer border-0 bg-danger-bg text-danger-text"
                onclick={() => deleteScannedSkill(skill.filePath, skill.name)}>Delete</button
              >
            </div>
          {/snippet}
          {@render skillRow(
            skill.filePath,
            skill.name,
            agentLabels[skill.agent] ?? skill.agent,
            details,
          )}
        {/each}
      </div>
    {/if}

    {#if globalScanned.length > 0}
      <span class="text-xs font-semibold text-text-secondary uppercase tracking-[0.4px]"
        >Global Skills</span
      >
      <div class="flex flex-col gap-1">
        {#each globalScanned as skill (skill.filePath)}
          {#snippet details()}
            <p class="text-sm text-text-secondary m-0 leading-snug">
              {skill.description || 'No description'}
            </p>
            <div class="flex gap-6">
              <div class="flex flex-col gap-0.5">
                <span class="text-xs font-semibold text-text-secondary uppercase tracking-[0.3px]"
                  >Agent</span
                >
                <span class="text-sm text-text">{agentLabels[skill.agent] ?? skill.agent}</span>
              </div>
              <div class="flex flex-col gap-0.5">
                <span class="text-xs font-semibold text-text-secondary uppercase tracking-[0.3px]"
                  >Scope</span
                >
                <span class="text-sm text-text">{skill.scope}</span>
              </div>
            </div>
            <div class="flex flex-col gap-0.5">
              <span class="text-xs font-semibold text-text-secondary uppercase tracking-[0.3px]"
                >File</span
              >
              <span class="text-sm text-text break-all">{skill.filePath}</span>
            </div>
            <div class="flex gap-2 pt-1">
              <button
                class="px-3.5 py-1 rounded-lg text-sm font-inherit cursor-pointer border-0 bg-danger-bg text-danger-text"
                onclick={() => deleteScannedSkill(skill.filePath, skill.name)}>Delete</button
              >
            </div>
          {/snippet}
          {@render skillRow(
            skill.filePath,
            skill.name,
            agentLabels[skill.agent] ?? skill.agent,
            details,
          )}
        {/each}
      </div>
    {/if}
  {/if}

  {#if skills.length === 0 && scannedSkills.length === 0 && !scanning}
    <p class="text-md text-text-faint m-0">
      No skills found. Install from a local path, URL, or GitHub, or scan to discover existing ones.
    </p>
  {/if}

  {#if scanning}
    <p class="text-md text-text-faint m-0">Scanning for skills...</p>
  {/if}

  {#if showInstallForm}
    <!-- svelte-ignore a11y_no_static_element_interactions -->
    <div
      class="flex flex-col gap-2 p-3 border border-border rounded-xl bg-border-subtle"
      onkeydown={(e) => {
        if (e.key === 'Enter') {
          e.preventDefault()
          installSkill()
        }
        if (e.key === 'Escape') showInstallForm = false
      }}
    >
      <input
        class="px-2.5 py-1.5 border border-border rounded-lg bg-hover text-text text-md font-inherit outline-none focus:border-focus-ring"
        bind:value={installSource}
        placeholder="Source (github:owner/repo/path, local path, or URL)"
      />

      <div class="flex items-center gap-2">
        <label class="text-sm text-text-secondary min-w-15">Agents:</label>
        <div class="flex gap-3 text-sm text-text">
          {#each Object.entries(agentLabels) as [key, label] (key)}
            <label class="flex items-center gap-1 cursor-pointer">
              <input
                type="checkbox"
                checked={installAgents.includes(key)}
                onchange={() => toggleInstallAgent(key)}
              />
              {label}
            </label>
          {/each}
        </div>
      </div>

      <div class="flex items-center gap-2">
        <label class="text-sm text-text-secondary min-w-15">Scope:</label>
        <div class="flex gap-3 text-sm text-text">
          <label><input type="radio" bind:group={installScope} value="project" /> Project</label>
          <label><input type="radio" bind:group={installScope} value="global" /> Global</label>
        </div>
      </div>

      <div class="flex items-center gap-2">
        <label class="text-sm text-text-secondary min-w-15">Method:</label>
        <div class="flex gap-3 text-sm text-text">
          <label><input type="radio" bind:group={installMethod} value="copy" /> Copy</label>
          <label><input type="radio" bind:group={installMethod} value="symlink" /> Symlink</label>
        </div>
      </div>

      {#if installError}
        <p class="text-sm text-danger m-0">{installError}</p>
      {/if}

      <div class="flex justify-end gap-2">
        <button
          class="px-3.5 py-1.5 rounded-lg text-md font-inherit cursor-pointer border-0 bg-active text-text"
          onclick={() => (showInstallForm = false)}>Cancel</button
        >
        <button
          class="px-3.5 py-1.5 rounded-lg text-md font-inherit cursor-pointer border-0 bg-accent-bg text-accent-text disabled:opacity-60 disabled:cursor-not-allowed hover:bg-accent-bg-hover"
          onclick={installSkill}
          disabled={installing}
        >
          {installing ? 'Installing...' : 'Install'}
        </button>
      </div>
    </div>
  {:else}
    <div class="flex gap-2">
      <button
        class="px-3.5 py-1.5 border border-dashed border-text-faint rounded-lg bg-transparent text-text-secondary text-md font-inherit cursor-pointer hover:bg-hover hover:text-text"
        onclick={() => {
          installSource = ''
          installAgents = ['claude']
          installScope = 'project'
          installMethod = 'copy'
          installError = ''
          showInstallForm = true
        }}>+ Install Skill</button
      >
      <button
        class="px-3.5 py-1.5 border border-dashed border-text-faint rounded-lg bg-transparent text-text-secondary text-md font-inherit cursor-pointer disabled:opacity-60 hover:bg-hover hover:text-text"
        onclick={scanForSkills}
        disabled={scanning}
      >
        {scanning ? 'Scanning...' : 'Rescan'}
      </button>
    </div>
  {/if}
</div>
