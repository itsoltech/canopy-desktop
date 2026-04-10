<script lang="ts">
  import { getSkills } from '../../lib/stores/skills.svelte'
  import { workspaceState } from '../../lib/stores/workspace.svelte'

  const skills = $derived(getSkills())

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
    } catch (e) {
      installError = e instanceof Error ? e.message : String(e)
    } finally {
      installing = false
    }
  }

  async function removeSkill(id: string, name: string): Promise<void> {
    if (!window.confirm(`Remove skill "${name}"?`)) return
    try {
      await window.api.removeSkill(id, workspaceState.selectedWorktreePath ?? undefined)
    } catch (e) {
      console.error('Failed to remove skill:', e)
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

<div class="section">
  <h3 class="section-title">Skills</h3>

  <div class="skill-list">
    {#each skills as skill (skill.id)}
      <div class="skill-row">
        <div
          class="skill-main"
          role="button"
          tabindex="0"
          onclick={() => toggleExpand(skill.id)}
          onkeydown={(e) => e.key === 'Enter' && toggleExpand(skill.id)}
        >
          <span class="skill-name">{skill.name}</span>
          <span class="skill-agents">
            {skill.enabledAgents.map((a) => agentLabels[a] ?? a).join(', ')}
          </span>
        </div>

        {#if expandedId === skill.id}
          <div class="skill-details">
            <p class="skill-description">{skill.description || 'No description'}</p>

            <div class="meta-item">
              <span class="meta-label">Source</span>
              <span class="meta-value">{skill.sourceUri}</span>
            </div>
            <div class="skill-meta-row">
              <div class="meta-item">
                <span class="meta-label">Scope</span>
                <span class="meta-value">{skill.scope}</span>
              </div>
              <div class="meta-item">
                <span class="meta-label">Type</span>
                <span class="meta-value">{skill.sourceType}</span>
              </div>
              <div class="meta-item">
                <span class="meta-label">Method</span>
                <span class="meta-value">{skill.installMethod}</span>
              </div>
              <div class="meta-item">
                <span class="meta-label">Version</span>
                <span class="meta-value">{skill.version}</span>
              </div>
            </div>

            <div class="agent-toggles">
              <span class="meta-label">Agents</span>
              <div class="agent-toggle-list">
                {#each skill.agents as agent (agent)}
                  <label class="agent-toggle">
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

            <div class="skill-actions">
              <button class="btn btn-action btn-update" onclick={() => updateSkill(skill.id)}
                >Update</button
              >
              <button
                class="btn btn-action btn-remove"
                onclick={() => removeSkill(skill.id, skill.name)}>Remove</button
              >
            </div>
          </div>
        {/if}
      </div>
    {/each}
  </div>

  {#if skills.length === 0}
    <p class="empty-state">
      No skills installed yet. Install from a local path, URL, or GitHub, or scan to discover
      existing ones.
    </p>
  {/if}

  {#if showInstallForm}
    <!-- svelte-ignore a11y_no_static_element_interactions -->
    <div
      class="install-form"
      onkeydown={(e) => {
        if (e.key === 'Enter') {
          e.preventDefault()
          installSkill()
        }
        if (e.key === 'Escape') showInstallForm = false
      }}
    >
      <input
        class="form-input"
        bind:value={installSource}
        placeholder="Source (github:owner/repo/path, local path, or URL)"
      />

      <div class="form-row">
        <label class="form-label">Agents:</label>
        <div class="agent-checkboxes">
          {#each Object.entries(agentLabels) as [key, label] (key)}
            <label class="agent-checkbox">
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

      <div class="form-row">
        <label class="form-label">Scope:</label>
        <div class="radio-group">
          <label><input type="radio" bind:group={installScope} value="project" /> Project</label>
          <label><input type="radio" bind:group={installScope} value="global" /> Global</label>
        </div>
      </div>

      <div class="form-row">
        <label class="form-label">Method:</label>
        <div class="radio-group">
          <label><input type="radio" bind:group={installMethod} value="copy" /> Copy</label>
          <label><input type="radio" bind:group={installMethod} value="symlink" /> Symlink</label>
        </div>
      </div>

      {#if installError}
        <p class="form-error">{installError}</p>
      {/if}

      <div class="form-actions">
        <button class="btn btn-cancel" onclick={() => (showInstallForm = false)}>Cancel</button>
        <button class="btn btn-install" onclick={installSkill} disabled={installing}>
          {installing ? 'Installing...' : 'Install'}
        </button>
      </div>
    </div>
  {:else}
    <div class="button-row">
      <button
        class="btn btn-add-skill"
        onclick={() => {
          installSource = ''
          installAgents = ['claude']
          installScope = 'project'
          installMethod = 'copy'
          installError = ''
          showInstallForm = true
        }}>+ Install Skill</button
      >
      <button class="btn btn-add-skill" onclick={scanForSkills} disabled={scanning}>
        {scanning ? 'Scanning...' : 'Scan for Skills'}
      </button>
    </div>
  {/if}

  {#if scannedSkills.length > 0}
    {@const projectSkills = scannedSkills.filter((s) => s.scope === 'project')}
    {@const globalSkills = scannedSkills.filter((s) => s.scope === 'global')}
    <div class="scan-section">
      <h4 class="scan-title">Discovered Skills</h4>
      <p class="scan-note">Found in agent directories on disk. Includes manually added skills.</p>

      {#if projectSkills.length > 0}
        <span class="scan-group-label">Project</span>
        <div class="skill-list">
          {#each projectSkills as skill (skill.filePath)}
            <div class="skill-row">
              <div class="skill-main">
                <span class="skill-name">{skill.name}</span>
                <span class="skill-agents">{skill.agent}</span>
              </div>
            </div>
          {/each}
        </div>
      {/if}

      {#if globalSkills.length > 0}
        <span class="scan-group-label">Global</span>
        <div class="skill-list">
          {#each globalSkills as skill (skill.filePath)}
            <div class="skill-row">
              <div class="skill-main">
                <span class="skill-name">{skill.name}</span>
                <span class="skill-agents">{skill.agent}</span>
              </div>
            </div>
          {/each}
        </div>
      {/if}
    </div>
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

  .skill-list {
    display: flex;
    flex-direction: column;
    gap: 4px;
  }

  .skill-row {
    border-radius: 6px;
    background: var(--c-border-subtle);
    overflow: hidden;
  }

  .skill-main {
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 6px 10px;
    font-size: 13px;
    cursor: pointer;
  }

  .skill-main:hover {
    background: var(--c-hover);
  }

  .skill-name {
    color: var(--c-text);
    min-width: 120px;
    font-weight: 500;
  }

  .skill-agents {
    color: var(--c-text-secondary);
    font-size: 12px;
    flex: 1;
  }

  .skill-details {
    padding: 10px 12px;
    border-top: 1px solid var(--c-border);
    display: flex;
    flex-direction: column;
    gap: 10px;
  }

  .skill-description {
    font-size: 12px;
    color: var(--c-text-secondary);
    margin: 0;
    line-height: 1.4;
  }

  .skill-meta-row {
    display: flex;
    gap: 24px;
  }

  .meta-item {
    display: flex;
    flex-direction: column;
    gap: 2px;
  }

  .meta-label {
    font-size: 11px;
    font-weight: 600;
    color: var(--c-text-secondary);
    text-transform: uppercase;
    letter-spacing: 0.3px;
  }

  .meta-value {
    font-size: 12px;
    color: var(--c-text);
    word-break: break-all;
  }

  .agent-toggles {
    display: flex;
    flex-direction: column;
    gap: 4px;
  }

  .agent-toggle-list {
    display: flex;
    gap: 14px;
  }

  .agent-toggle {
    font-size: 13px;
    color: var(--c-text);
    display: flex;
    align-items: center;
    gap: 6px;
    cursor: pointer;
  }

  .skill-actions {
    display: flex;
    gap: 8px;
    padding-top: 4px;
  }

  .empty-state {
    font-size: 13px;
    color: var(--c-text-faint);
    margin: 0;
  }

  .install-form {
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

  .form-row {
    display: flex;
    align-items: center;
    gap: 8px;
  }

  .form-label {
    font-size: 12px;
    color: var(--c-text-secondary);
    min-width: 60px;
  }

  .agent-checkboxes,
  .radio-group {
    display: flex;
    gap: 12px;
    font-size: 12px;
    color: var(--c-text);
  }

  .agent-checkbox {
    display: flex;
    align-items: center;
    gap: 4px;
    cursor: pointer;
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

  .btn-install {
    background: var(--c-accent-bg);
    color: var(--c-accent-text);
  }

  .btn-install:hover {
    background: var(--c-accent-bg-hover);
  }

  .btn-install:disabled {
    opacity: 0.6;
    cursor: not-allowed;
  }

  .button-row {
    display: flex;
    gap: 8px;
  }

  .btn-add-skill {
    padding: 6px 14px;
    border: 1px dashed var(--c-text-faint);
    border-radius: 6px;
    background: transparent;
    color: var(--c-text-secondary);
    font-size: 13px;
    font-family: inherit;
    cursor: pointer;
  }

  .btn-add-skill:hover {
    background: var(--c-hover);
    color: var(--c-text);
  }

  .scan-section {
    display: flex;
    flex-direction: column;
    gap: 8px;
  }

  .scan-group-label {
    font-size: 11px;
    font-weight: 600;
    color: var(--c-text-secondary);
    text-transform: uppercase;
    letter-spacing: 0.4px;
    margin-top: 4px;
  }

  .scan-title {
    font-size: 13px;
    font-weight: 600;
    color: var(--c-text-secondary);
    margin: 0;
  }

  .scan-note {
    font-size: 12px;
    color: var(--c-text-faint);
    margin: 0;
  }

  .scan-scope {
    font-size: 10px;
    color: var(--c-text-muted);
    text-transform: uppercase;
    letter-spacing: 0.5px;
  }

  .btn-action {
    font-size: 12px;
    padding: 5px 14px;
  }

  .btn-update {
    background: var(--c-accent-bg);
    color: var(--c-accent-text);
  }

  .btn-update:hover {
    background: var(--c-accent-bg-hover);
  }

  .btn-remove {
    background: var(--c-danger-bg);
    color: var(--c-danger-text);
  }
</style>
