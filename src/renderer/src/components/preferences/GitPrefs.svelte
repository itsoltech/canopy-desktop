<script lang="ts">
  import { prefs, setPref, getPref } from '../../lib/stores/preferences.svelte'
  import CustomRadio from '../shared/CustomRadio.svelte'
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

<div class="flex flex-col gap-4">
  <h3 class="text-[15px] font-semibold text-text m-0">Git</h3>

  <div class="flex flex-col gap-2">
    <span class="text-sm font-medium text-text-secondary uppercase tracking-[0.5px]"
      >Pull Strategy</span
    >
    <span class="text-xs text-text-faint">
      How local commits are integrated when pulling remote changes
    </span>
    <div class="flex flex-col gap-1.5">
      <label class="flex items-center gap-2 text-md text-text">
        <CustomRadio checked={pullRebase} onchange={() => setPullStrategy(true)} />
        <span>Rebase</span>
        <span class="text-xs text-text-muted font-mono">git pull --rebase</span>
      </label>
      <label class="flex items-center gap-2 text-md text-text">
        <CustomRadio checked={!pullRebase} onchange={() => setPullStrategy(false)} />
        <span>Merge</span>
        <span class="text-xs text-text-muted font-mono">git pull</span>
      </label>
    </div>
  </div>

  <div class="flex flex-col gap-2">
    <span class="text-sm font-medium text-text-secondary uppercase tracking-[0.5px]"
      >Worktrees directory</span
    >
    <input
      class="w-full border border-border rounded-lg bg-bg-input text-text text-md font-inherit px-2.5 py-2 outline-none transition-colors duration-fast box-border focus:border-focus-ring placeholder:text-text-faint"
      type="text"
      value={worktreesDir}
      oninput={(e) => updateWorktreesDir(e.currentTarget.value)}
      placeholder="~/canopy/worktrees"
      spellcheck="false"
      autocomplete="off"
    />
    <span class="text-xs text-text-faint">Pattern: &lt;dir&gt;/&lt;project&gt;/&lt;branch&gt;</span>
  </div>

  {#if showSetup}
    <div class="flex flex-col gap-2">
      <span class="text-sm font-medium text-text-secondary uppercase tracking-[0.5px]"
        >Worktree Setup</span
      >
      <span class="text-xs text-text-faint">Actions to run after creating a new worktree</span>

      {#if actions.length > 0}
        <div class="flex flex-col gap-1.5">
          {#each actions as action, i (i)}
            <div class="flex items-center gap-1.5">
              <span class="text-xs font-mono text-accent-text min-w-8 flex-shrink-0"
                >{action.type === 'command' ? 'run' : 'copy'}</span
              >
              {#if action.type === 'command'}
                <input
                  class="flex-1 border border-border rounded-md bg-bg-input text-text text-sm font-mono px-2 py-1 outline-none focus:border-focus-ring placeholder:text-text-faint box-border"
                  type="text"
                  bind:value={action.command}
                  oninput={() => updateAction()}
                  placeholder="npm install"
                  spellcheck="false"
                  autocomplete="off"
                />
              {:else}
                <input
                  class="flex-1 min-w-0 border border-border rounded-md bg-bg-input text-text text-sm font-mono px-2 py-1 outline-none focus:border-focus-ring placeholder:text-text-faint box-border"
                  type="text"
                  bind:value={action.source}
                  oninput={() => updateAction()}
                  placeholder=".env"
                  spellcheck="false"
                  autocomplete="off"
                />
                <span class="text-sm text-text-faint flex-shrink-0">→</span>
                <input
                  class="flex-1 min-w-0 border border-border rounded-md bg-bg-input text-text text-sm font-mono px-2 py-1 outline-none focus:border-focus-ring placeholder:text-text-faint box-border"
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
              <button
                class="bg-transparent border-0 text-text-faint text-base cursor-pointer px-1 leading-none flex-shrink-0 transition-colors duration-fast hover:text-danger-text"
                onclick={() => removeAction(i)}
                title="Remove">×</button
              >
            </div>
          {/each}
        </div>
      {/if}

      <div class="flex gap-2">
        <button
          class="bg-hover border border-border rounded-md text-text-secondary text-sm font-inherit px-2.5 py-1 cursor-pointer transition-colors duration-fast hover:bg-hover-strong hover:text-text"
          onclick={addCommand}>+ command</button
        >
        <button
          class="bg-hover border border-border rounded-md text-text-secondary text-sm font-inherit px-2.5 py-1 cursor-pointer transition-colors duration-fast hover:bg-hover-strong hover:text-text"
          onclick={addCopy}>+ file copy</button
        >
      </div>

      {#if actions.some((a) => a.type === 'command')}
        <span class="text-xs text-text-faint">
          Variables: $MAIN_WORKTREE, $NEW_WORKTREE, $REPO_ROOT
        </span>
      {/if}
    </div>
  {/if}
</div>
