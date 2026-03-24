<script lang="ts">
  import { prefs, setPref, getPref } from '../../lib/stores/preferences.svelte'
  import { workspaceState } from '../../lib/stores/workspace.svelte'

  let pullRebase = $derived(prefs.gitPullRebase !== 'false')
  let worktreesDir = $state(getPref('worktrees.baseDir', ''))

  function setPullStrategy(rebase: boolean): void {
    setPref('gitPullRebase', rebase ? 'true' : 'false')
  }

  function updateWorktreesDir(value: string): void {
    worktreesDir = value
    if (value.trim()) {
      setPref('worktrees.baseDir', value.trim())
    } else {
      setPref('worktrees.baseDir', '')
    }
  }

  // --- Worktree Setup Actions ---

  interface SetupCommandAction {
    type: 'command'
    command: string
    label?: string
  }

  interface SetupCopyAction {
    type: 'copy'
    source: string
    dest?: string
    label?: string
  }

  type SetupAction = SetupCommandAction | SetupCopyAction

  const workspaceId = $derived(workspaceState.workspace?.id)
  const prefKey = $derived(workspaceId ? `workspace:${workspaceId}:worktreeSetup` : null)
  const showSetup = $derived(workspaceState.isGitRepo && !!workspaceId)

  function loadActions(): SetupAction[] {
    if (!prefKey) return []
    const raw = getPref(prefKey, '')
    if (!raw) return []
    try {
      return JSON.parse(raw) as SetupAction[]
    } catch {
      return []
    }
  }

  let actions = $state<SetupAction[]>(loadActions())

  $effect(() => {
    if (prefKey) {
      actions = loadActions()
    }
  })

  function persistActions(): void {
    if (!prefKey) return
    setPref(prefKey, JSON.stringify(actions))
  }

  function addCommand(): void {
    actions.push({ type: 'command', command: '' })
    persistActions()
  }

  function addCopy(): void {
    actions.push({ type: 'copy', source: '' })
    persistActions()
  }

  function removeAction(index: number): void {
    actions.splice(index, 1)
    persistActions()
  }

  function updateAction(): void {
    persistActions()
  }
</script>

<div class="section">
  <h3 class="section-title">Git</h3>

  <div class="field">
    <span class="field-label">Pull Strategy</span>
    <div class="radio-group">
      <label class="radio-row">
        <input
          type="radio"
          name="pull-strategy"
          checked={pullRebase}
          onchange={() => setPullStrategy(true)}
        />
        <span>Rebase</span>
        <span class="radio-desc">git pull --rebase</span>
      </label>
      <label class="radio-row">
        <input
          type="radio"
          name="pull-strategy"
          checked={!pullRebase}
          onchange={() => setPullStrategy(false)}
        />
        <span>Merge</span>
        <span class="radio-desc">git pull</span>
      </label>
    </div>
  </div>

  <div class="field">
    <span class="field-label">Worktrees directory</span>
    <input
      class="field-input"
      type="text"
      value={worktreesDir}
      oninput={(e) => updateWorktreesDir(e.currentTarget.value)}
      placeholder="~/canopy/worktrees"
      spellcheck="false"
      autocomplete="off"
    />
    <span class="field-hint">Pattern: &lt;dir&gt;/&lt;project&gt;/&lt;branch&gt;</span>
  </div>

  {#if showSetup}
    <div class="field">
      <span class="field-label">Worktree Setup</span>
      <span class="field-hint">Actions to run after creating a new worktree</span>

      {#if actions.length > 0}
        <div class="setup-list">
          {#each actions as action, i (i)}
            <div class="setup-item">
              <span class="setup-type">{action.type === 'command' ? 'run' : 'copy'}</span>
              {#if action.type === 'command'}
                <input
                  class="setup-input"
                  type="text"
                  bind:value={action.command}
                  oninput={() => updateAction()}
                  placeholder="npm install"
                  spellcheck="false"
                  autocomplete="off"
                />
              {:else}
                <input
                  class="setup-input setup-input-short"
                  type="text"
                  bind:value={action.source}
                  oninput={() => updateAction()}
                  placeholder=".env"
                  spellcheck="false"
                  autocomplete="off"
                />
                <span class="setup-arrow">→</span>
                <input
                  class="setup-input setup-input-short"
                  type="text"
                  value={action.dest ?? action.source}
                  oninput={(e) => {
                    const val = e.currentTarget.value
                    action.dest = val === action.source ? undefined : val
                    updateAction()
                  }}
                  placeholder=".env"
                  spellcheck="false"
                  autocomplete="off"
                />
              {/if}
              <button class="setup-remove" onclick={() => removeAction(i)} title="Remove">×</button>
            </div>
          {/each}
        </div>
      {/if}

      <div class="setup-actions">
        <button class="setup-add" onclick={addCommand}>+ command</button>
        <button class="setup-add" onclick={addCopy}>+ file copy</button>
      </div>

      {#if actions.some((a) => a.type === 'command')}
        <span class="field-hint"> Variables: $MAIN_WORKTREE, $NEW_WORKTREE, $REPO_ROOT </span>
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
    color: #e0e0e0;
    margin: 0;
  }

  .field {
    display: flex;
    flex-direction: column;
    gap: 8px;
  }

  .field-label {
    font-size: 12px;
    font-weight: 500;
    color: rgba(255, 255, 255, 0.5);
    text-transform: uppercase;
    letter-spacing: 0.5px;
  }

  .radio-group {
    display: flex;
    flex-direction: column;
    gap: 6px;
  }

  .radio-row {
    display: flex;
    align-items: center;
    gap: 8px;
    font-size: 13px;
    color: rgba(255, 255, 255, 0.8);
    cursor: pointer;
  }

  .radio-row input[type='radio'] {
    accent-color: #74c0fc;
  }

  .radio-desc {
    font-size: 11px;
    color: rgba(255, 255, 255, 0.35);
    font-family: monospace;
  }

  .field-input {
    width: 100%;
    border: 1px solid rgba(255, 255, 255, 0.12);
    border-radius: 6px;
    background: rgba(0, 0, 0, 0.3);
    color: #e0e0e0;
    font-size: 13px;
    font-family: inherit;
    padding: 8px 10px;
    outline: none;
    transition: border-color 0.1s;
    box-sizing: border-box;
  }

  .field-input:focus {
    border-color: rgba(116, 192, 252, 0.5);
  }

  .field-input::placeholder {
    color: rgba(255, 255, 255, 0.25);
  }

  .field-hint {
    font-size: 11px;
    color: rgba(255, 255, 255, 0.3);
  }

  .setup-list {
    display: flex;
    flex-direction: column;
    gap: 6px;
  }

  .setup-item {
    display: flex;
    align-items: center;
    gap: 6px;
  }

  .setup-type {
    font-size: 11px;
    font-family: monospace;
    color: rgba(116, 192, 252, 0.7);
    min-width: 32px;
    flex-shrink: 0;
  }

  .setup-input {
    flex: 1;
    border: 1px solid rgba(255, 255, 255, 0.12);
    border-radius: 4px;
    background: rgba(0, 0, 0, 0.3);
    color: #e0e0e0;
    font-size: 12px;
    font-family: monospace;
    padding: 5px 8px;
    outline: none;
    transition: border-color 0.1s;
    box-sizing: border-box;
  }

  .setup-input:focus {
    border-color: rgba(116, 192, 252, 0.5);
  }

  .setup-input::placeholder {
    color: rgba(255, 255, 255, 0.25);
  }

  .setup-input-short {
    flex: 1;
    min-width: 0;
  }

  .setup-arrow {
    font-size: 12px;
    color: rgba(255, 255, 255, 0.3);
    flex-shrink: 0;
  }

  .setup-remove {
    background: none;
    border: none;
    color: rgba(255, 255, 255, 0.3);
    font-size: 16px;
    cursor: pointer;
    padding: 0 4px;
    line-height: 1;
    flex-shrink: 0;
    transition: color 0.1s;
  }

  .setup-remove:hover {
    color: rgba(255, 120, 120, 0.9);
  }

  .setup-actions {
    display: flex;
    gap: 8px;
  }

  .setup-add {
    background: rgba(255, 255, 255, 0.06);
    border: 1px solid rgba(255, 255, 255, 0.1);
    border-radius: 4px;
    color: rgba(255, 255, 255, 0.5);
    font-size: 12px;
    font-family: inherit;
    padding: 4px 10px;
    cursor: pointer;
    transition:
      background 0.1s,
      color 0.1s;
  }

  .setup-add:hover {
    background: rgba(255, 255, 255, 0.1);
    color: rgba(255, 255, 255, 0.8);
  }
</style>
