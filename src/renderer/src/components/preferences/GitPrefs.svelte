<script lang="ts">
  import { prefs, setPref, getPref } from '../../lib/stores/preferences.svelte'
  import CustomRadio from '../shared/CustomRadio.svelte'
  import { workspaceState } from '../../lib/stores/workspace.svelte'
  import PrefsSection from './_partials/PrefsSection.svelte'
  import PrefsRow from './_partials/PrefsRow.svelte'

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

<div class="flex flex-col gap-7">
  <PrefsSection title="Pull strategy" description="How local commits are integrated when pulling">
    <PrefsRow
      label="Rebase"
      help="Replay local commits on top of upstream — keeps history linear"
      search="git pull rebase linear history"
    >
      <CustomRadio checked={pullRebase} onchange={() => setPullStrategy(true)} />
    </PrefsRow>
    <PrefsRow
      label="Merge"
      help="Create a merge commit when histories diverge"
      search="git pull merge commit"
    >
      <CustomRadio checked={!pullRebase} onchange={() => setPullStrategy(false)} />
    </PrefsRow>
  </PrefsSection>

  <PrefsSection
    title="Worktrees"
    description="Where new git worktrees are created and what runs after"
  >
    <PrefsRow
      label="Base directory"
      help="Pattern: <dir>/<project>/<branch>"
      search="worktree directory base path"
      layout="stacked"
    >
      <input
        class="w-full px-2.5 py-1.5 border border-border rounded-md bg-bg-input text-text text-md font-inherit outline-none focus:border-focus-ring placeholder:text-text-faint"
        type="text"
        name="worktreesBaseDir"
        aria-label="Worktrees base directory"
        value={worktreesDir}
        oninput={(e) => updateWorktreesDir(e.currentTarget.value)}
        placeholder="~/canopy/worktrees"
        spellcheck="false"
        autocomplete="off"
      />
    </PrefsRow>

    {#if showSetup}
      <PrefsRow
        label="Setup actions"
        help="Run after creating a new worktree. Variables: $MAIN_WORKTREE, $NEW_WORKTREE, $REPO_ROOT"
        search="worktree setup post-create command copy bootstrap"
        layout="stacked"
      >
        <div class="flex flex-col gap-2">
          {#if actions.length > 0}
            <div class="flex flex-col gap-1.5">
              {#each actions as action, i (i)}
                <div class="flex items-center gap-1.5">
                  <span
                    class="text-xs font-mono text-accent-text min-w-8 shrink-0 uppercase tracking-caps-tight"
                  >
                    {action.type === 'command' ? 'run' : 'copy'}
                  </span>
                  {#if action.type === 'command'}
                    <input
                      class="flex-1 min-w-0 border border-border rounded-md bg-bg-input text-text text-sm font-mono px-2 py-1 outline-none focus:border-focus-ring placeholder:text-text-faint"
                      type="text"
                      name="worktreeSetupCommand"
                      aria-label="Worktree setup command"
                      bind:value={action.command}
                      oninput={() => updateAction()}
                      placeholder="npm install"
                      spellcheck="false"
                      autocomplete="off"
                    />
                  {:else}
                    <input
                      class="flex-1 min-w-0 border border-border rounded-md bg-bg-input text-text text-sm font-mono px-2 py-1 outline-none focus:border-focus-ring placeholder:text-text-faint"
                      type="text"
                      name="worktreeSetupCopySource"
                      aria-label="Worktree setup copy source"
                      bind:value={action.source}
                      oninput={() => updateAction()}
                      placeholder=".env"
                      spellcheck="false"
                      autocomplete="off"
                    />
                    <span class="text-sm text-text-faint shrink-0">→</span>
                    <input
                      class="flex-1 min-w-0 border border-border rounded-md bg-bg-input text-text text-sm font-mono px-2 py-1 outline-none focus:border-focus-ring placeholder:text-text-faint"
                      type="text"
                      name="worktreeSetupCopyDest"
                      aria-label="Worktree setup copy destination"
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
                    type="button"
                    class="bg-transparent border-0 text-text-faint text-base cursor-pointer px-1 leading-none shrink-0 hover:text-danger-text"
                    onclick={() => removeAction(i)}
                    aria-label="Remove action"
                    title="Remove">×</button
                  >
                </div>
              {/each}
            </div>
          {/if}

          <div class="flex gap-2">
            <button
              type="button"
              class="bg-hover border border-border rounded-md text-text-secondary text-sm font-inherit px-2.5 py-1 cursor-pointer hover:bg-hover-strong hover:text-text"
              onclick={addCommand}>+ command</button
            >
            <button
              type="button"
              class="bg-hover border border-border rounded-md text-text-secondary text-sm font-inherit px-2.5 py-1 cursor-pointer hover:bg-hover-strong hover:text-text"
              onclick={addCopy}>+ file copy</button
            >
          </div>
        </div>
      </PrefsRow>
    {/if}
  </PrefsSection>
</div>
