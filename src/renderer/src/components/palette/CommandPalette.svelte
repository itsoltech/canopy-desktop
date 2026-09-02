<script lang="ts">
  import { onMount, onDestroy } from 'svelte'
  import { SvelteMap } from 'svelte/reactivity'
  import {
    Search,
    Wrench,
    GitBranch,
    GitCommitVertical,
    PanelLeft,
    SlidersVertical,
    Terminal,
  } from '@lucide/svelte'
  import {
    workspaceState,
    toggleSidebar,
    toggleRightPanel,
    openWorkspace,
    selectWorktree,
  } from '../../lib/stores/workspace.svelte'
  import {
    openTool,
    switchTab,
    getAllTabs,
    getTabsForWorktree,
    getTabDisplayName,
    reopenClosedTab,
    splitFocusedPane,
    closeAllTabsForWorktree,
  } from '../../lib/stores/tabs.svelte'
  import {
    confirm,
    prompt,
    showCreateWorktree,
    showPreferences,
    showAbout,
    showTmuxBrowser,
  } from '../../lib/stores/dialogs.svelte'
  import { getTools, getToolAvailability } from '../../lib/stores/tools.svelte'
  import { addToast } from '../../lib/stores/toast.svelte'
  import { confirmWorktreeRemoval } from '../../lib/worktrees/removalConsent'

  let { onClose }: { onClose: () => void } = $props()

  interface PaletteItem {
    id: string
    label: string
    category: string
    description?: string
    shortcut?: string
    disabled?: boolean
    action: () => void | Promise<void>
  }

  let query = $state('')
  let selectedIndex = $state(0)
  let inputEl: HTMLInputElement | undefined = $state()

  let tmuxAvailable = $state(false)

  // Captured during init, before the palette takes focus. Restored only if
  // nothing else claimed focus meanwhile — running a command can open a tab or
  // terminal that focuses itself, and we must not steal focus back from it.
  const previouslyFocused = document.activeElement as HTMLElement | null
  onDestroy(() => {
    if (document.activeElement === document.body) previouslyFocused?.focus?.()
  })

  onMount(() => {
    inputEl?.focus()
    window.api
      .tmuxIsAvailable()
      .then((v) => (tmuxAvailable = v))
      .catch(() => {})
  })

  const isMac = navigator.userAgent.includes('Mac')
  const mod = isMac ? 'Cmd' : 'Ctrl'

  const CATEGORY_ICON: Record<string, typeof Search> = {
    Tools: Wrench,
    Git: GitCommitVertical,
    Worktrees: GitBranch,
    Tabs: PanelLeft,
    App: SlidersVertical,
    Terminal,
  }

  async function closeAllOpenTabsForWorktree(path: string): Promise<void> {
    await closeAllTabsForWorktree(path)
  }

  let allItems = $derived.by((): PaletteItem[] => {
    const items: PaletteItem[] = []
    const path = workspaceState.selectedWorktreePath

    for (const tool of getTools()) {
      const available = getToolAvailability()[tool.id] !== false
      items.push({
        id: `tool:${tool.id}`,
        label: `Launch ${tool.name}`,
        category: 'Tools',
        disabled: !available || !path,
        description: !available ? 'Not found in PATH' : undefined,
        action: () => {
          if (path && available) openTool(tool.id, path)
        },
      })
    }

    if (workspaceState.isGitRepo) {
      for (const wt of workspaceState.worktrees) {
        const branchName = wt.branch || wt.path.split('/').pop() || wt.path
        const isCurrent = wt.path === workspaceState.selectedWorktreePath
        items.push({
          id: `wt:${wt.path}`,
          label: branchName,
          category: 'Worktrees',
          description: isCurrent ? 'current' : wt.path,
          disabled: isCurrent,
          action: () => {
            if (!isCurrent) selectWorktree(wt.path)
          },
        })
      }
    }

    const openTabs = getAllTabs()
    for (const tab of openTabs) {
      items.push({
        id: `tab:${tab.id}`,
        label: getTabDisplayName(tab),
        category: 'Tabs',
        description: tab.worktreePath.split('/').pop() || '',
        action: async () => await switchTab(tab.id),
      })
    }

    items.push({
      id: 'app:sidebar',
      label: 'Toggle Sidebar',
      category: 'App',
      shortcut: `${mod}+B`,
      action: () => toggleSidebar(),
    })

    items.push({
      id: 'app:inspector',
      label: 'Toggle Claude Inspector',
      category: 'App',
      shortcut: `${mod}+Shift+I`,
      action: () => toggleRightPanel(),
    })

    items.push({
      id: 'app:preferences',
      label: 'Preferences',
      category: 'App',
      shortcut: `${mod}+,`,
      action: () => showPreferences(),
    })

    items.push({
      id: 'app:new-window',
      label: 'New Window',
      category: 'App',
      shortcut: `${mod}+Shift+N`,
      action: () => window.api.newWindow(),
    })

    items.push({
      id: 'app:about',
      label: 'About Canopy',
      category: 'App',
      action: () => showAbout(),
    })

    if (tmuxAvailable) {
      items.push({
        id: 'tmux:sessions',
        label: 'Tmux Sessions (Experimental)',
        category: 'Terminal',
        description: 'Browse and manage tmux sessions — unstable, dev-only',
        action: () => showTmuxBrowser(),
      })
    }

    items.push({
      id: 'app:open-folder',
      label: 'Open Folder',
      category: 'App',
      shortcut: `${mod}+O`,
      action: async () => {
        const p = await window.api.openFolder()
        if (p) await openWorkspace(p)
      },
    })

    if (path) {
      const currentWorktreeTabs = getTabsForWorktree(path)

      items.push({
        id: 'app:new-shell',
        label: 'New Shell Tab',
        category: 'App',
        shortcut: `${mod}+T`,
        action: () => openTool('shell', path),
      })

      items.push({
        id: 'app:open-notes',
        label: 'Open Notes',
        category: 'App',
        description: 'Markdown scratch pad (in-memory, per worktree or project)',
        action: () => openTool('notes', path),
      })

      items.push({
        id: 'app:open-drawing',
        label: 'Open Drawing',
        category: 'App',
        description: 'Freehand sketchpad — send to active agent',
        action: () => openTool('drawing', path),
      })

      items.push({
        id: 'app:reopen-tab',
        label: 'Reopen Closed Tab',
        category: 'App',
        shortcut: `${mod}+Shift+T`,
        action: () => reopenClosedTab(path),
      })

      if (currentWorktreeTabs.length > 0) {
        items.push({
          id: 'app:close-worktree-tabs',
          label: 'Close All Tabs in Worktree',
          category: 'App',
          description: `Close ${currentWorktreeTabs.length} open tab${
            currentWorktreeTabs.length === 1 ? '' : 's'
          } in the active worktree`,
          action: () => closeAllOpenTabsForWorktree(path),
        })
      }

      items.push({
        id: 'app:split-vertical',
        label: 'Split Pane Vertical',
        category: 'App',
        shortcut: `${mod}+D`,
        action: () => splitFocusedPane(path, 'vsplit'),
      })

      items.push({
        id: 'app:split-horizontal',
        label: 'Split Pane Horizontal',
        category: 'App',
        shortcut: `${mod}+Shift+D`,
        action: () => splitFocusedPane(path, 'hsplit'),
      })
    }

    if (workspaceState.isGitRepo && workspaceState.repoRoot) {
      const root = workspaceState.repoRoot

      items.push({
        id: 'git:commit',
        label: 'Commit',
        category: 'Git',
        action: async () => {
          const result = await prompt({
            title: 'Commit',
            placeholder: 'Commit message...',
            multiline: true,
            submitLabel: 'Commit',
            onGenerate: () => window.api.gitGenerateCommitMessage(root),
            checkbox: { label: 'Stage all changes', checked: true },
          })
          if (!result) return
          try {
            await window.api.gitCommitWorktree({
              repoRoot: root,
              message: result.value,
              stageAll: result.checked,
            })
          } catch (err) {
            await confirm({
              title: 'Git Error',
              message: err instanceof Error ? err.message : String(err),
              confirmLabel: 'OK',
            })
          }
        },
      })

      items.push({
        id: 'git:push',
        label: 'Push',
        category: 'Git',
        action: async () => {
          try {
            const preflight = await window.api.gitPreparePush({ repoRoot: root })
            const ok = await confirm({
              title: 'Push',
              message: preflight.confirmationMessage,
            })
            if (ok) {
              await window.api.gitPushWorktree({ repoRoot: root })
            }
          } catch (err) {
            await confirm({
              title: 'Git Error',
              message: err instanceof Error ? err.message : String(err),
              confirmLabel: 'OK',
            })
          }
        },
      })

      items.push({
        id: 'git:pull',
        label: 'Pull',
        category: 'Git',
        action: async () => {
          try {
            await window.api.gitPullWithPreferences({ repoRoot: root })
          } catch (err) {
            await confirm({
              title: 'Git Error',
              message: err instanceof Error ? err.message : String(err),
              confirmLabel: 'OK',
            })
          }
        },
      })

      items.push({
        id: 'git:fetch',
        label: 'Fetch',
        category: 'Git',
        action: async () => {
          try {
            await window.api.gitFetchWorktree({ repoRoot: root })
          } catch (err) {
            await confirm({
              title: 'Git Error',
              message: err instanceof Error ? err.message : String(err),
              confirmLabel: 'OK',
            })
          }
        },
      })

      items.push({
        id: 'git:stash',
        label: 'Stash',
        category: 'Git',
        action: async () => {
          try {
            await window.api.gitStashWorktree({ repoRoot: root })
          } catch (err) {
            await confirm({
              title: 'Git Error',
              message: err instanceof Error ? err.message : String(err),
              confirmLabel: 'OK',
            })
          }
        },
      })

      items.push({
        id: 'git:stashPop',
        label: 'Stash Pop',
        category: 'Git',
        action: async () => {
          try {
            await window.api.gitStashPopWorktree({ repoRoot: root })
          } catch (err) {
            await confirm({
              title: 'Git Error',
              message: err instanceof Error ? err.message : String(err),
              confirmLabel: 'OK',
            })
          }
        },
      })

      items.push({
        id: 'git:branchCreate',
        label: 'Create Branch',
        category: 'Git',
        action: async () => {
          const result = await prompt({
            title: 'Create Branch',
            placeholder: 'Branch name...',
            submitLabel: 'Create',
            validate: (v) => {
              if (/\s/.test(v)) return 'No spaces allowed'
              if (/\.\./.test(v)) return 'Cannot contain ..'
              if (/[~^:\\]/.test(v)) return 'Invalid characters'
              if (v.startsWith('-')) return 'Cannot start with -'
              return null
            },
          })
          if (!result) return
          try {
            await window.api.gitBranchCreateFromHead({ repoRoot: root, branch: result.value })
          } catch (err) {
            await confirm({
              title: 'Git Error',
              message: err instanceof Error ? err.message : String(err),
              confirmLabel: 'OK',
            })
          }
        },
      })

      items.push({
        id: 'git:branchDelete',
        label: 'Delete Branch',
        category: 'Git',
        action: async () => {
          const result = await prompt({
            title: 'Delete Branch',
            placeholder: 'Branch name to delete...',
            submitLabel: 'Delete',
          })
          if (!result) return
          try {
            const preflight = await window.api.gitBranchPrepareDelete({
              repoRoot: root,
              branch: result.value,
            })
            if (preflight.forceRequired) {
              const force = await confirm({
                title: 'Delete Unmerged Branch',
                message: `Branch "${result.value}" has not been fully merged.\nForce delete?`,
                confirmLabel: 'Force Delete',
                destructive: true,
              })
              if (!force) return
            }
            await window.api.gitBranchDeleteWithPreflight({
              repoRoot: root,
              branch: result.value,
              forceIfUnmerged: preflight.forceRequired,
            })
          } catch (err) {
            await confirm({
              title: 'Git Error',
              message: err instanceof Error ? err.message : String(err),
              confirmLabel: 'OK',
            })
          }
        },
      })

      items.push({
        id: 'git:worktreeCreate',
        label: 'Create Worktree',
        category: 'Git',
        action: () => showCreateWorktree(),
      })

      const selectedWt = workspaceState.worktrees.find(
        (w) => w.path === workspaceState.selectedWorktreePath,
      )
      if (selectedWt && !selectedWt.isMain) {
        items.push({
          id: 'git:worktreeRemove',
          label: 'Remove Current Worktree',
          category: 'Git',
          action: async () => {
            const wtPath = selectedWt.path
            const branch = selectedWt.branch
            const operationRoot = workspaceState.worktrees.find((w) => w.isMain)?.path ?? root
            try {
              // Shared preflight/consent gate (same as the sidebar flows): warnings
              // and destructive consent are collected BEFORE any teardown.
              const consent = await confirmWorktreeRemoval({
                repoRoot: operationRoot,
                worktreePath: wtPath,
                branch,
              })
              if (!consent.ok) return

              let deleteBranch = false
              if (consent.preflight?.canDeleteBranch) {
                deleteBranch = await confirm({
                  title: 'Delete Branch?',
                  message: `Delete local branch "${branch}"? It has been fully merged.`,
                  confirmLabel: 'Delete Branch',
                })
              }

              // This command removes the CURRENT worktree: close its tabs and
              // leave it BEFORE the removal, otherwise live shells and watchers
              // hold Windows file locks inside the directory being deleted.
              if (!(await closeAllTabsForWorktree(wtPath, { forRemoval: true }))) return
              const main = workspaceState.worktrees.find((w) => w.isMain)
              if (main) selectWorktree(main.path)

              const result = await window.api.worktreeRemoveWithBranch({
                repoRoot: operationRoot,
                worktreePath: wtPath,
                branch,
                deleteBranch,
                // Lock retries and cleanup run unconditionally in the main flow;
                // this flag only authorizes --force for dirty/force-required
                // refusals, so it must carry exactly the consent collected above.
                forceOnFailure: consent.force,
              })
              if (result.leftoverPath) {
                addToast(
                  `Worktree removed, but some files are still in use and were left at ${result.leftoverPath}`,
                )
              }
            } catch (err) {
              await confirm({
                title: 'Git Error',
                message: err instanceof Error ? err.message : String(err),
                confirmLabel: 'OK',
              })
            }
          },
        })
      }
    }

    return items
  })

  let filterMode = $derived.by((): 'all' | 'app' | 'git' => {
    if (query.startsWith('>')) return 'app'
    if (query.toLowerCase().startsWith('git ')) return 'git'
    return 'all'
  })

  let searchQuery = $derived.by((): string => {
    if (filterMode === 'app') return query.slice(1).trim().toLowerCase()
    if (filterMode === 'git') return query.slice(4).trim().toLowerCase()
    return query.trim().toLowerCase()
  })

  function fuzzyMatch(text: string, q: string): boolean {
    if (!q) return true
    const lower = text.toLowerCase()
    let qi = 0
    for (let i = 0; i < lower.length && qi < q.length; i++) {
      if (lower[i] === q[qi]) qi++
    }
    return qi === q.length
  }

  let filteredItems = $derived.by((): PaletteItem[] => {
    let source = allItems

    if (filterMode === 'app') {
      source = source.filter((i) => i.category === 'App')
    } else if (filterMode === 'git') {
      source = source.filter((i) => i.category === 'Git')
    }

    if (!searchQuery) return source
    return source.filter(
      (item) => fuzzyMatch(item.label, searchQuery) || fuzzyMatch(item.category, searchQuery),
    )
  })

  let groupedItems = $derived.by((): { category: string; items: PaletteItem[] }[] => {
    const categoryOrder = ['Tools', 'Git', 'Worktrees', 'Tabs', 'App']
    const groups = new SvelteMap<string, PaletteItem[]>()

    for (const item of filteredItems) {
      const list = groups.get(item.category) ?? []
      list.push(item)
      groups.set(item.category, list)
    }

    return categoryOrder
      .filter((c) => groups.has(c))
      .map((c) => ({ category: c, items: groups.get(c)! }))
  })

  let flatItems = $derived(groupedItems.flatMap((g) => g.items))

  $effect(() => {
    if (selectedIndex >= flatItems.length) {
      selectedIndex = Math.max(0, flatItems.length - 1)
    }
  })

  function executeSelected(): void {
    const item = flatItems[selectedIndex]
    if (item && !item.disabled) {
      onClose()
      item.action()
    }
  }

  function handleKeydown(e: KeyboardEvent): void {
    if (e.key === 'Escape') {
      e.preventDefault()
      onClose()
      return
    }

    if (e.key === 'Tab') {
      // The search input is the only focusable control; keep focus trapped in
      // the dialog instead of letting Tab move it to background window chrome.
      e.preventDefault()
      inputEl?.focus()
      return
    }

    if (e.key === 'ArrowDown') {
      e.preventDefault()
      selectedIndex = (selectedIndex + 1) % Math.max(1, flatItems.length)
      scrollSelectedIntoView()
      return
    }

    if (e.key === 'ArrowUp') {
      e.preventDefault()
      selectedIndex = (selectedIndex - 1 + flatItems.length) % Math.max(1, flatItems.length)
      scrollSelectedIntoView()
      return
    }

    if (e.key === 'Enter') {
      e.preventDefault()
      executeSelected()
      return
    }
  }

  function scrollSelectedIntoView(): void {
    requestAnimationFrame(() => {
      const el = document.querySelector('[data-palette-selected="true"]')
      el?.scrollIntoView({ block: 'nearest' })
    })
  }

  $effect(() => {
    // eslint-disable-next-line @typescript-eslint/no-unused-expressions
    query
    selectedIndex = 0
  })

  function flatIndex(categoryIdx: number, itemIdx: number): number {
    let idx = 0
    for (let i = 0; i < categoryIdx; i++) {
      idx += groupedItems[i].items.length
    }
    return idx + itemIdx
  }
</script>

<!-- svelte-ignore a11y_no_static_element_interactions -->
<div
  class="fixed inset-0 z-[1000] flex justify-center pt-20 bg-scrim"
  onkeydown={handleKeydown}
  onclick={onClose}
>
  <!-- svelte-ignore a11y_click_events_have_key_events -->
  <div
    class="w-[520px] max-h-[440px] flex flex-col bg-bg-overlay border border-border-subtle rounded-lg shadow-modal backdrop-blur-2xl overflow-hidden self-start"
    role="dialog"
    aria-modal="true"
    aria-label="Command palette"
    onclick={(e) => e.stopPropagation()}
  >
    <div class="h-12 px-3 flex items-center gap-2 border-b border-border-subtle flex-shrink-0">
      <Search size={14} class="text-text-faint flex-shrink-0" />
      <input
        bind:this={inputEl}
        bind:value={query}
        class="w-full border-0 bg-transparent text-text text-md font-inherit outline-none placeholder:text-text-faint"
        type="text"
        role="combobox"
        aria-label="Search commands"
        aria-controls="command-palette-listbox"
        aria-expanded={flatItems.length > 0}
        aria-activedescendant={flatItems.length > 0 && selectedIndex >= 0
          ? `command-palette-option-${selectedIndex}`
          : undefined}
        placeholder={filterMode === 'app'
          ? 'Search app commands...'
          : filterMode === 'git'
            ? 'Search git commands...'
            : 'Search tools, tabs, worktrees, git...'}
        spellcheck="false"
        autocomplete="off"
      />
    </div>

    <div
      id="command-palette-listbox"
      role="listbox"
      aria-label="Command palette results"
      class="overflow-y-auto flex-1 py-1.5"
    >
      {#if flatItems.length === 0}
        <div class="px-3 py-5 text-center text-text-faint text-md" role="status">No results</div>
      {:else}
        {#each groupedItems as group, gi (group.category)}
          {@const Icon = CATEGORY_ICON[group.category] ?? SlidersVertical}
          <div class="py-0.5">
            <div
              class="flex items-center gap-1.5 text-2xs font-semibold tracking-caps-looser text-text-faint px-3 pt-2 pb-1 uppercase leading-tight"
            >
              <Icon size={11} class="flex-shrink-0" />
              <span>{group.category}</span>
            </div>
            {#each group.items as item, ii (item.id)}
              {@const isSelected = flatIndex(gi, ii) === selectedIndex}
              {@const isShortDesc = !!item.description && item.description.length <= 24}
              <!-- svelte-ignore a11y_click_events_have_key_events -->
              <div
                id={`command-palette-option-${flatIndex(gi, ii)}`}
                role="option"
                aria-selected={isSelected}
                aria-disabled={item.disabled}
                class="flex items-center gap-2 h-8 px-3 cursor-pointer text-md text-text transition-colors duration-fast"
                class:bg-accent-bg={isSelected}
                class:text-accent-text={isSelected}
                class:opacity-50={item.disabled}
                class:cursor-default={item.disabled}
                data-palette-selected={isSelected}
                onclick={() => {
                  if (!item.disabled) {
                    onClose()
                    item.action()
                  }
                }}
                onpointerenter={() => {
                  if (!item.disabled) selectedIndex = flatIndex(gi, ii)
                }}
              >
                <span
                  class="flex-1 overflow-hidden text-ellipsis whitespace-nowrap"
                  title={item.label}>{item.label}</span
                >
                {#if item.description}
                  {#if isShortDesc}
                    <span
                      class="inline-flex items-center h-5 px-1.5 rounded-sm text-2xs font-semibold tracking-caps-tight uppercase flex-shrink-0 {isSelected
                        ? 'bg-accent-bg-hover text-accent-text'
                        : 'bg-border-subtle text-text-faint'}"
                    >
                      {item.description}
                    </span>
                  {:else}
                    <span
                      class="text-xs flex-shrink-0 {isSelected
                        ? 'text-accent-text'
                        : 'text-text-faint'}"
                    >
                      {item.description}
                    </span>
                  {/if}
                {/if}
                {#if item.shortcut}
                  <span
                    class="inline-flex items-center h-5 px-1.5 rounded-sm text-2xs font-mono font-semibold flex-shrink-0 {isSelected
                      ? 'bg-accent-bg-hover text-accent-text'
                      : 'bg-border-subtle text-text-secondary'}"
                  >
                    {item.shortcut}
                  </span>
                {/if}
              </div>
            {/each}
          </div>
        {/each}
      {/if}
    </div>
  </div>
</div>
