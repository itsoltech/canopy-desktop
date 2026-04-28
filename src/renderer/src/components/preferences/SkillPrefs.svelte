<script lang="ts">
  import { onMount } from 'svelte'
  import { Plus, RotateCw, Trash2 } from '@lucide/svelte'
  import { getSkills } from '../../lib/stores/skills.svelte'
  import { confirm } from '../../lib/stores/dialogs.svelte'
  import { workspaceState } from '../../lib/stores/workspace.svelte'
  import CustomCheckbox from '../shared/CustomCheckbox.svelte'
  import PrefsSection from './_partials/PrefsSection.svelte'
  import SkillRow from './_partials/SkillRow.svelte'
  import SkillInstallForm from './_partials/SkillInstallForm.svelte'
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

  function rowVisible(haystack: string): boolean {
    if (prefsSearch.query.trim() === '') return true
    return matches(haystack)
  }
</script>

{#snippet detailField(label: string, value: string, mono = false)}
  <div class="flex flex-col gap-0.5">
    <span class="text-2xs font-semibold uppercase tracking-caps-tight text-text-faint">{label}</span
    >
    <span class="text-sm text-text break-all" class:font-mono={mono}>{value}</span>
  </div>
{/snippet}

{#snippet scannedDetails(skill: (typeof scannedSkills)[number])}
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

{#snippet scannedSection(title: string, description: string, list: typeof scannedSkills)}
  <PrefsSection {title} {description}>
    <div class="flex flex-col">
      {#each list as skill (skill.filePath)}
        <SkillRow
          name={skill.name}
          agentDisplay={agentLabels[skill.agent] ?? skill.agent}
          expanded={expandedId === skill.filePath}
          dimmed={!rowVisible(
            `${skill.name} ${agentLabels[skill.agent] ?? skill.agent} ${skill.description ?? ''} ${skill.filePath} ${skill.scope}`,
          )}
          onToggle={() => toggleExpand(skill.filePath)}
        >
          {@render scannedDetails(skill)}
        </SkillRow>
      {/each}
    </div>
  </PrefsSection>
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
          <SkillRow
            name={skill.name}
            {agentDisplay}
            expanded={expandedId === skill.id}
            dimmed={!rowVisible(
              `${skill.name} ${agentDisplay} ${skill.description ?? ''} ${skill.sourceUri ?? ''} ${skill.scope ?? ''}`,
            )}
            onToggle={() => toggleExpand(skill.id)}
          >
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
          </SkillRow>
        {/each}
      </div>
    </PrefsSection>
  {/if}

  {#if projectScanned.length > 0}
    {@render scannedSection('Project skills', 'Discovered in this workspace', projectScanned)}
  {/if}

  {#if globalScanned.length > 0}
    {@render scannedSection('Global skills', 'Discovered in your home directory', globalScanned)}
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
      <SkillInstallForm
        {agentLabels}
        workspacePath={workspaceState.selectedWorktreePath ?? undefined}
        onClose={() => (showInstallForm = false)}
        onInstalled={scanForSkills}
      />
    {:else}
      <div class="flex gap-2">
        <button
          type="button"
          class="flex items-center gap-1 px-3 py-1 rounded-md bg-border-subtle border border-border text-text-secondary text-sm font-inherit cursor-pointer hover:bg-active hover:text-text"
          onclick={() => (showInstallForm = true)}
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
