<script lang="ts">
  import { onMount, type Snippet } from 'svelte'
  import { ChevronDown, ChevronRight, Plus, RotateCw, Trash2 } from '@lucide/svelte'
  import { getSkills } from '../../lib/stores/skills.svelte'
  import { confirm } from '../../lib/stores/dialogs.svelte'
  import { workspaceState } from '../../lib/stores/workspace.svelte'
  import CustomCheckbox from '../shared/CustomCheckbox.svelte'
  import CustomRadio from '../shared/CustomRadio.svelte'
  import PrefsSection from './_partials/PrefsSection.svelte'
  import { prefsSearch, matches } from './_partials/prefsSearch.svelte'

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

  function resetInstallForm(): void {
    installSource = ''
    installAgents = ['claude']
    installScope = 'project'
    installMethod = 'copy'
    installError = ''
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
      resetInstallForm()
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
      title: 'Remove skill',
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
      title: 'Delete skill file',
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

  function rowVisible(haystack: string): boolean {
    if (prefsSearch.query.trim() === '') return true
    return matches(haystack)
  }
</script>

{#snippet skillRow(
  id: string,
  name: string,
  agentDisplay: string,
  search: string,
  details: Snippet,
)}
  {@const expanded = expandedId === id}
  <div
    class="flex flex-col border-t border-border-subtle first:border-t-0 transition-opacity duration-fast"
    class:opacity-30={!rowVisible(`${name} ${agentDisplay} ${search}`)}
  >
    <button
      type="button"
      class="flex items-center gap-2 w-full px-1 py-2 border-0 bg-transparent text-left cursor-pointer hover:bg-row-hover rounded-md"
      onclick={() => toggleExpand(id)}
    >
      {#if expanded}
        <ChevronDown size={13} class="shrink-0 text-text-muted" />
      {:else}
        <ChevronRight size={13} class="shrink-0 text-text-muted" />
      {/if}
      <span class="text-md text-text min-w-30 truncate">{name}</span>
      <span class="text-xs text-text-muted truncate flex-1">{agentDisplay}</span>
    </button>
    {#if expanded}
      <div class="pl-6 pr-1 pb-3 flex flex-col gap-2.5">
        {@render details()}
      </div>
    {/if}
  </div>
{/snippet}

{#snippet detailField(label: string, value: string, mono = false)}
  <div class="flex flex-col gap-0.5">
    <span class="text-2xs font-semibold uppercase tracking-caps-tight text-text-faint">{label}</span
    >
    <span class="text-sm text-text break-all" class:font-mono={mono}>{value}</span>
  </div>
{/snippet}

<div class="flex flex-col gap-7">
  {#if skills.length > 0}
    <PrefsSection
      title="Installed skills"
      description="Skills registered with the Canopy skill store"
    >
      <div class="flex flex-col">
        {#each skills as skill (skill.id)}
          {@const agentDisplay = skill.enabledAgents.map((a) => agentLabels[a] ?? a).join(', ')}
          {#snippet details()}
            <p class="text-sm text-text-secondary m-0 leading-snug">
              {skill.description || 'No description'}
            </p>
            {@render detailField('Source', skill.sourceUri, true)}
            <div class="flex flex-wrap gap-x-6 gap-y-2">
              {@render detailField('Scope', skill.scope)}
              {@render detailField('Type', skill.sourceType)}
              {@render detailField('Method', skill.installMethod)}
              {@render detailField('Version', skill.version)}
            </div>
            <div class="flex flex-col gap-1">
              <span class="text-2xs font-semibold uppercase tracking-caps-tight text-text-faint"
                >Agents</span
              >
              <div class="flex gap-4 flex-wrap">
                {#each skill.agents as agent (agent)}
                  <label class="flex items-center gap-2 text-md text-text cursor-pointer">
                    <CustomCheckbox
                      checked={skill.enabledAgents.includes(agent)}
                      onchange={() =>
                        toggleAgent(skill.id, agent, !skill.enabledAgents.includes(agent))}
                    />
                    <span>{agentLabels[agent] ?? agent}</span>
                  </label>
                {/each}
              </div>
            </div>
            <div class="flex gap-2 pt-1">
              <button
                type="button"
                class="flex items-center gap-1 px-3 py-1 rounded-md text-sm font-inherit cursor-pointer border-0 bg-accent-bg text-accent-text hover:bg-accent-bg-hover"
                onclick={() => updateSkill(skill.id)}
              >
                <RotateCw size={12} /> Update
              </button>
              <button
                type="button"
                class="flex items-center gap-1 px-3 py-1 rounded-md text-sm font-inherit cursor-pointer border-0 bg-danger-bg text-danger-text"
                onclick={() => removeSkill(skill.id, skill.name)}
              >
                <Trash2 size={12} /> Remove
              </button>
            </div>
          {/snippet}
          {@render skillRow(
            skill.id,
            skill.name,
            agentDisplay,
            `${skill.description ?? ''} ${skill.sourceUri ?? ''} ${skill.scope ?? ''}`,
            details,
          )}
        {/each}
      </div>
    </PrefsSection>
  {/if}

  {#if projectScanned.length > 0}
    <PrefsSection title="Project skills" description="Discovered in this workspace">
      <div class="flex flex-col">
        {#each projectScanned as skill (skill.filePath)}
          {#snippet details()}
            <p class="text-sm text-text-secondary m-0 leading-snug">
              {skill.description || 'No description'}
            </p>
            <div class="flex flex-wrap gap-x-6 gap-y-2">
              {@render detailField('Agent', agentLabels[skill.agent] ?? skill.agent)}
              {@render detailField('Scope', skill.scope)}
            </div>
            {@render detailField('File', skill.filePath, true)}
            <div class="flex gap-2 pt-1">
              <button
                type="button"
                class="flex items-center gap-1 px-3 py-1 rounded-md text-sm font-inherit cursor-pointer border-0 bg-danger-bg text-danger-text"
                onclick={() => deleteScannedSkill(skill.filePath, skill.name)}
              >
                <Trash2 size={12} /> Delete file
              </button>
            </div>
          {/snippet}
          {@render skillRow(
            skill.filePath,
            skill.name,
            agentLabels[skill.agent] ?? skill.agent,
            `${skill.description ?? ''} ${skill.filePath} ${skill.scope}`,
            details,
          )}
        {/each}
      </div>
    </PrefsSection>
  {/if}

  {#if globalScanned.length > 0}
    <PrefsSection title="Global skills" description="Discovered in your home directory">
      <div class="flex flex-col">
        {#each globalScanned as skill (skill.filePath)}
          {#snippet details()}
            <p class="text-sm text-text-secondary m-0 leading-snug">
              {skill.description || 'No description'}
            </p>
            <div class="flex flex-wrap gap-x-6 gap-y-2">
              {@render detailField('Agent', agentLabels[skill.agent] ?? skill.agent)}
              {@render detailField('Scope', skill.scope)}
            </div>
            {@render detailField('File', skill.filePath, true)}
            <div class="flex gap-2 pt-1">
              <button
                type="button"
                class="flex items-center gap-1 px-3 py-1 rounded-md text-sm font-inherit cursor-pointer border-0 bg-danger-bg text-danger-text"
                onclick={() => deleteScannedSkill(skill.filePath, skill.name)}
              >
                <Trash2 size={12} /> Delete file
              </button>
            </div>
          {/snippet}
          {@render skillRow(
            skill.filePath,
            skill.name,
            agentLabels[skill.agent] ?? skill.agent,
            `${skill.description ?? ''} ${skill.filePath} ${skill.scope}`,
            details,
          )}
        {/each}
      </div>
    </PrefsSection>
  {/if}

  {#if skills.length === 0 && scannedSkills.length === 0 && !scanning}
    <p class="text-sm text-text-muted m-0">
      No skills found. Install from a local path, URL, or GitHub, or scan to discover existing ones.
    </p>
  {/if}

  {#if scanning}
    <p class="text-sm text-text-muted m-0">Scanning for skills…</p>
  {/if}

  <PrefsSection title="Install">
    {#if showInstallForm}
      <!-- svelte-ignore a11y_no_static_element_interactions -->
      <div
        class="flex flex-col gap-2 p-3 border border-border rounded-md bg-bg-input"
        onkeydown={(e) => {
          if (e.key === 'Enter') {
            e.preventDefault()
            installSkill()
          }
          if (e.key === 'Escape') {
            showInstallForm = false
            resetInstallForm()
          }
        }}
      >
        <input
          class="px-2.5 py-1.5 border border-border rounded-md bg-bg text-text text-md font-inherit outline-none focus:border-focus-ring placeholder:text-text-faint"
          name="installSource"
          aria-label="Skill source"
          bind:value={installSource}
          placeholder="github:owner/repo/path, local path, or URL"
          spellcheck="false"
          autocomplete="off"
        />

        <div class="flex flex-col gap-1.5">
          <span class="text-2xs font-semibold uppercase tracking-caps-tight text-text-faint"
            >Agents</span
          >
          <div class="flex gap-3 flex-wrap">
            {#each Object.entries(agentLabels) as [key, label] (key)}
              <label class="flex items-center gap-1.5 text-md text-text cursor-pointer">
                <CustomCheckbox
                  checked={installAgents.includes(key)}
                  onchange={() => toggleInstallAgent(key)}
                />
                <span>{label}</span>
              </label>
            {/each}
          </div>
        </div>

        <div class="flex flex-col gap-1.5">
          <span class="text-2xs font-semibold uppercase tracking-caps-tight text-text-faint"
            >Scope</span
          >
          <div class="flex gap-3 flex-wrap">
            <label class="flex items-center gap-1.5 text-md text-text cursor-pointer">
              <CustomRadio
                checked={installScope === 'project'}
                onchange={() => (installScope = 'project')}
              />
              <span>Project</span>
            </label>
            <label class="flex items-center gap-1.5 text-md text-text cursor-pointer">
              <CustomRadio
                checked={installScope === 'global'}
                onchange={() => (installScope = 'global')}
              />
              <span>Global</span>
            </label>
          </div>
        </div>

        <div class="flex flex-col gap-1.5">
          <span class="text-2xs font-semibold uppercase tracking-caps-tight text-text-faint"
            >Method</span
          >
          <div class="flex gap-3 flex-wrap">
            <label class="flex items-center gap-1.5 text-md text-text cursor-pointer">
              <CustomRadio
                checked={installMethod === 'copy'}
                onchange={() => (installMethod = 'copy')}
              />
              <span>Copy</span>
            </label>
            <label class="flex items-center gap-1.5 text-md text-text cursor-pointer">
              <CustomRadio
                checked={installMethod === 'symlink'}
                onchange={() => (installMethod = 'symlink')}
              />
              <span>Symlink</span>
            </label>
          </div>
        </div>

        {#if installError}
          <p class="text-sm text-danger-text m-0">{installError}</p>
        {/if}

        <div class="flex justify-end gap-2">
          <button
            type="button"
            class="px-3 py-1 rounded-md text-sm font-inherit cursor-pointer border border-border bg-transparent text-text-secondary hover:bg-hover hover:text-text"
            onclick={() => {
              showInstallForm = false
              resetInstallForm()
            }}>Cancel</button
          >
          <button
            type="button"
            class="px-3 py-1 rounded-md text-sm font-inherit cursor-pointer border-0 bg-accent-bg text-accent-text disabled:opacity-60 disabled:cursor-default hover:bg-accent-bg-hover"
            onclick={installSkill}
            disabled={installing}
          >
            {installing ? 'Installing…' : 'Install'}
          </button>
        </div>
      </div>
    {:else}
      <div class="flex gap-2">
        <button
          type="button"
          class="flex items-center gap-1 px-3 py-1 rounded-md bg-border-subtle border border-border text-text-secondary text-sm font-inherit cursor-pointer hover:bg-active hover:text-text"
          onclick={() => {
            resetInstallForm()
            showInstallForm = true
          }}
        >
          <Plus size={12} />
          <span>Install skill</span>
        </button>
        <button
          type="button"
          class="flex items-center gap-1 px-3 py-1 rounded-md bg-transparent border border-border text-text-secondary text-sm font-inherit cursor-pointer hover:bg-hover hover:text-text disabled:opacity-60 disabled:cursor-default"
          onclick={scanForSkills}
          disabled={scanning}
        >
          <RotateCw size={12} />
          <span>{scanning ? 'Scanning…' : 'Rescan'}</span>
        </button>
      </div>
    {/if}
  </PrefsSection>
</div>
