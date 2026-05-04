<script lang="ts">
  import { FolderGit2, Terminal, FileText, ArrowRight, Trash2, Plus } from '@lucide/svelte'
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

  const workspace = $derived(workspaceState.workspace)
  const workspaceId = $derived(workspace?.id)
  const prefKey = $derived(workspaceId ? `workspace:${workspaceId}:worktreeSetup` : null)

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
    } else {
      actions = []
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

  const hasCommandAction = $derived(actions.some((a) => a.type === 'command'))
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

  <PrefsSection title="Worktrees" description="Where new git worktrees are created">
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
  </PrefsSection>

  <PrefsSection
    title="Worktree setup"
    description="Bootstrap commands and file copies that run after `git worktree add`. Stored with the active project — not shared with other workspaces."
    badge={{ text: 'Project', tone: 'accent' }}
  >
    {#if !workspace}
      <div class="px-3 py-3 rounded-md bg-bg-input border border-dashed border-border-subtle">
        <p class="text-sm text-text-muted m-0">
          Open a project to configure its worktree setup actions.
        </p>
      </div>
    {:else if !workspaceState.isGitRepo}
      <div class="px-3 py-3 rounded-md bg-bg-input border border-dashed border-border-subtle">
        <p class="text-sm text-text-muted m-0">
          <strong class="text-text-secondary font-medium">{workspace.name}</strong> isn't a git repository.
          Initialize git in this project to use worktree setup actions.
        </p>
      </div>
    {:else}
      <div class="flex flex-col gap-4">
        <div
          class="flex items-start gap-2.5 px-3 py-2.5 rounded-md bg-bg-input border border-border-subtle"
        >
          <FolderGit2 size={14} class="shrink-0 text-accent mt-0.5" />
          <div class="flex flex-col gap-0.5 min-w-0 flex-1">
            <span class="text-md text-text truncate" title={workspace.name}>{workspace.name}</span>
            {#if workspaceState.repoRoot}
              <span
                class="text-2xs text-text-faint font-mono truncate"
                title={workspaceState.repoRoot}>{workspaceState.repoRoot}</span
              >
            {/if}
          </div>
          <span class="text-2xs uppercase tracking-caps-tight text-text-faint shrink-0 mt-0.5"
            >Saved in this project</span
          >
        </div>

        {#if actions.length > 0}
          <div class="flex flex-col gap-2">
            {#each actions as action, i (i)}
              <div
                class="flex items-center gap-2 px-2 py-1.5 rounded-md bg-bg-input border border-border-subtle"
              >
                {#if action.type === 'command'}
                  <span
                    class="flex items-center gap-1 text-2xs font-semibold uppercase tracking-caps-tight text-success-text shrink-0 min-w-13"
                    title="Run shell command"
                  >
                    <Terminal size={11} /> Run
                  </span>
                  <input
                    class="flex-1 min-w-0 border border-border rounded-md bg-bg text-text text-sm font-mono px-2 py-1 outline-none focus:border-focus-ring placeholder:text-text-faint"
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
                  <span
                    class="flex items-center gap-1 text-2xs font-semibold uppercase tracking-caps-tight text-accent-text shrink-0 min-w-13"
                    title="Copy file from main worktree"
                  >
                    <FileText size={11} /> Copy
                  </span>
                  <input
                    class="flex-1 min-w-0 border border-border rounded-md bg-bg text-text text-sm font-mono px-2 py-1 outline-none focus:border-focus-ring placeholder:text-text-faint"
                    type="text"
                    name="worktreeSetupCopySource"
                    aria-label="Worktree setup copy source"
                    bind:value={action.source}
                    oninput={() => updateAction()}
                    placeholder=".env"
                    spellcheck="false"
                    autocomplete="off"
                  />
                  <ArrowRight size={12} class="shrink-0 text-text-faint" />
                  <input
                    class="flex-1 min-w-0 border border-border rounded-md bg-bg text-text text-sm font-mono px-2 py-1 outline-none focus:border-focus-ring placeholder:text-text-faint"
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
                  class="flex items-center justify-center size-6 rounded-md bg-transparent border-0 text-text-muted cursor-pointer shrink-0 hover:bg-danger-bg hover:text-danger-text"
                  onclick={() => removeAction(i)}
                  aria-label="Remove action"
                  title="Remove"
                >
                  <Trash2 size={12} />
                </button>
              </div>
            {/each}
          </div>
        {:else}
          <p class="text-sm text-text-muted m-0 leading-snug">
            No actions yet. Add a command (e.g. <code class="font-mono text-text-secondary"
              >npm install</code
            >) or copy a file (e.g.
            <code class="font-mono text-text-secondary">.env</code>) to bootstrap new worktrees.
          </p>
        {/if}

        <div class="flex gap-2">
          <button
            type="button"
            class="flex items-center gap-1 px-3 py-1 rounded-md bg-border-subtle border border-border text-text-secondary text-sm font-inherit cursor-pointer hover:bg-active hover:text-text"
            onclick={addCommand}
          >
            <Plus size={12} />
            <Terminal size={12} />
            <span>Run command</span>
          </button>
          <button
            type="button"
            class="flex items-center gap-1 px-3 py-1 rounded-md bg-border-subtle border border-border text-text-secondary text-sm font-inherit cursor-pointer hover:bg-active hover:text-text"
            onclick={addCopy}
          >
            <Plus size={12} />
            <FileText size={12} />
            <span>Copy file</span>
          </button>
        </div>

        {#if hasCommandAction}
          <div class="flex flex-wrap items-center gap-1.5 pt-3 border-t border-border-subtle">
            <span class="text-2xs uppercase tracking-caps-tight text-text-faint mr-0.5"
              >Variables</span
            >
            <code
              class="text-xs font-mono px-1.5 py-px rounded-sm bg-bg-input border border-border-subtle text-accent-text"
              >$MAIN_WORKTREE</code
            >
            <code
              class="text-xs font-mono px-1.5 py-px rounded-sm bg-bg-input border border-border-subtle text-accent-text"
              >$NEW_WORKTREE</code
            >
            <code
              class="text-xs font-mono px-1.5 py-px rounded-sm bg-bg-input border border-border-subtle text-accent-text"
              >$REPO_ROOT</code
            >
          </div>
        {/if}
      </div>
    {/if}
  </PrefsSection>
</div>
