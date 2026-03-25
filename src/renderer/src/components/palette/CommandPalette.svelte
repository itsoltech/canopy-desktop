<script lang="ts">
  import { onMount } from 'svelte'
  import { SvelteMap } from 'svelte/reactivity'
  import {
    workspaceState,
    toggleSidebar,
    toggleInspector,
    openWorkspace,
    selectWorktree,
  } from '../../lib/stores/workspace.svelte'
  import {
    openTool,
    switchTab,
    getAllTabs,
    getTabDisplayName,
    reopenClosedTab,
    splitFocusedPane,
  } from '../../lib/stores/tabs.svelte'
  import {
    confirm,
    prompt,
    showCreateWorktree,
    showPreferences,
    showAbout,
  } from '../../lib/stores/dialogs.svelte'

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

  let tools: { id: string; name: string; category: string }[] = $state([])
  let availability: Record<string, boolean> = $state({})

  onMount(async () => {
    inputEl?.focus()
    const [toolList, avail] = await Promise.all([
      window.api.listTools(),
      window.api.checkToolAvailability(),
    ])
    tools = toolList
    availability = avail
  })

  const isMac = navigator.userAgent.includes('Mac')
  const mod = isMac ? 'Cmd' : 'Ctrl'

  // Build action list from all sources
  let allItems = $derived.by((): PaletteItem[] => {
    const items: PaletteItem[] = []
    const path = workspaceState.selectedWorktreePath

    // --- Tools ---
    for (const tool of tools) {
      const available = availability[tool.id] !== false
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

    // --- Worktrees ---
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

    // --- Tabs ---
    const openTabs = getAllTabs()
    for (const tab of openTabs) {
      items.push({
        id: `tab:${tab.id}`,
        label: getTabDisplayName(tab),
        category: 'Tabs',
        description: tab.worktreePath.split('/').pop() || '',
        action: () => switchTab(tab.id),
      })
    }

    // --- App actions ---
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
      action: () => toggleInspector(),
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
      items.push({
        id: 'app:new-shell',
        label: 'New Shell Tab',
        category: 'App',
        shortcut: `${mod}+T`,
        action: () => openTool('shell', path),
      })

      items.push({
        id: 'app:reopen-tab',
        label: 'Reopen Closed Tab',
        category: 'App',
        shortcut: `${mod}+Shift+T`,
        action: () => reopenClosedTab(path),
      })

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

    // --- Git commands ---
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
            await window.api.gitCommit(root, result.value, result.checked)
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
            const info = await window.api.gitPushInfo(root)
            if (!info) {
              await confirm({
                title: 'Push',
                message: 'No upstream tracking branch configured.',
                confirmLabel: 'OK',
              })
              return
            }
            const ok = await confirm({
              title: 'Push',
              message: `Push ${info.commitCount} commit(s) to ${info.remote}/${info.branch}?`,
            })
            if (ok) {
              await window.api.gitPush(root)
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
            const rebase = (await window.api.getPref('gitPullRebase')) !== 'false'
            await window.api.gitPull(root, rebase)
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
            await window.api.gitFetch(root)
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
            await window.api.gitStash(root)
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
            await window.api.gitStashPop(root)
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
            await window.api.gitBranchCreate(root, result.value, 'HEAD')
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
            const merged = await window.api.gitBranchMerged(root, result.value)
            if (!merged) {
              const force = await confirm({
                title: 'Delete Unmerged Branch',
                message: `Branch "${result.value}" has not been fully merged.\nForce delete?`,
                confirmLabel: 'Force Delete',
                destructive: true,
              })
              if (!force) return
              await window.api.gitBranchDelete(root, result.value, true)
            } else {
              await window.api.gitBranchDelete(root, result.value, false)
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
        id: 'git:worktreeCreate',
        label: 'Create Worktree',
        category: 'Git',
        action: () => showCreateWorktree(),
      })

      // Only show remove worktree if selected worktree is not main
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
            try {
              const status = await window.api.gitStatusPorcelain(root, wtPath)
              const unmerged = await window.api.gitUnmergedCommits(root, branch)

              const warnings: string[] = []
              if (status.trim()) warnings.push('Has uncommitted changes.')
              if (unmerged.length > 0)
                warnings.push(`${unmerged.length} unmerged commit(s) not on any remote.`)

              const msg =
                warnings.length > 0
                  ? warnings.join('\n') + '\n\nRemove this worktree?'
                  : `Remove worktree "${branch}"?`

              const ok = await confirm({
                title: 'Remove Worktree',
                message: msg,
                details: wtPath,
                confirmLabel: 'Remove',
                destructive: warnings.length > 0,
              })
              if (!ok) return

              await window.api.gitWorktreeRemove(root, wtPath, warnings.length > 0)

              // Offer to delete branch if merged
              const branchMerged = await window.api.gitBranchMerged(root, branch)
              if (branchMerged) {
                const del = await confirm({
                  title: 'Delete Branch?',
                  message: `Delete local branch "${branch}"? It has been fully merged.`,
                  confirmLabel: 'Delete Branch',
                })
                if (del) {
                  await window.api.gitBranchDelete(root, branch, false)
                }
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

  // Determine if we're in a prefix-filter mode
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

  // Fuzzy-ish matching: all query chars must appear in order in the label
  function fuzzyMatch(text: string, q: string): boolean {
    if (!q) return true
    const lower = text.toLowerCase()
    let qi = 0
    for (let i = 0; i < lower.length && qi < q.length; i++) {
      if (lower[i] === q[qi]) qi++
    }
    return qi === q.length
  }

  // Filtered + grouped items
  let filteredItems = $derived.by((): PaletteItem[] => {
    let source = allItems

    // Category filter based on prefix mode
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

  // Group by category for display
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

  // Flat list for keyboard navigation
  let flatItems = $derived(groupedItems.flatMap((g) => g.items))

  // Clamp selected index when filter changes
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
      const el = document.querySelector('.palette-item.selected')
      el?.scrollIntoView({ block: 'nearest' })
    })
  }

  // Reset selection when query changes
  $effect(() => {
    // eslint-disable-next-line @typescript-eslint/no-unused-expressions
    query
    selectedIndex = 0
  })

  // Flat index tracking for rendering
  function flatIndex(categoryIdx: number, itemIdx: number): number {
    let idx = 0
    for (let i = 0; i < categoryIdx; i++) {
      idx += groupedItems[i].items.length
    }
    return idx + itemIdx
  }
</script>

<!-- svelte-ignore a11y_no_static_element_interactions -->
<div class="palette-overlay" onkeydown={handleKeydown} onclick={onClose}>
  <!-- svelte-ignore a11y_click_events_have_key_events -->
  <div class="palette-container" onclick={(e) => e.stopPropagation()}>
    <div class="palette-input-row">
      <input
        bind:this={inputEl}
        bind:value={query}
        class="palette-input"
        type="text"
        placeholder={filterMode === 'app'
          ? 'Search app commands...'
          : filterMode === 'git'
            ? 'Search git commands...'
            : 'Search tools, tabs, worktrees, git...'}
        spellcheck="false"
        autocomplete="off"
      />
    </div>

    <div class="palette-results">
      {#if flatItems.length === 0}
        <div class="palette-empty">No results</div>
      {:else}
        {#each groupedItems as group, gi (group.category)}
          <div class="palette-group">
            <div class="palette-group-label">{group.category}</div>
            {#each group.items as item, ii (item.id)}
              <!-- svelte-ignore a11y_click_events_have_key_events -->
              <!-- svelte-ignore a11y_no_static_element_interactions -->
              <div
                class="palette-item"
                class:selected={flatIndex(gi, ii) === selectedIndex}
                class:disabled={item.disabled}
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
                <span class="item-label">{item.label}</span>
                {#if item.description}
                  <span class="item-desc">{item.description}</span>
                {/if}
                {#if item.shortcut}
                  <span class="item-shortcut">{item.shortcut}</span>
                {/if}
              </div>
            {/each}
          </div>
        {/each}
      {/if}
    </div>
  </div>
</div>

<style>
  .palette-overlay {
    position: fixed;
    inset: 0;
    z-index: 1000;
    display: flex;
    justify-content: center;
    padding-top: 80px;
    background: rgba(0, 0, 0, 0.5);
  }

  .palette-container {
    width: 520px;
    max-height: 400px;
    display: flex;
    flex-direction: column;
    background: rgba(30, 30, 30, 0.98);
    border: 1px solid rgba(255, 255, 255, 0.12);
    border-radius: 10px;
    box-shadow: 0 16px 48px rgba(0, 0, 0, 0.6);
    overflow: hidden;
    align-self: flex-start;
  }

  .palette-input-row {
    padding: 12px 14px;
    border-bottom: 1px solid rgba(255, 255, 255, 0.08);
    flex-shrink: 0;
  }

  .palette-input {
    width: 100%;
    border: none;
    background: transparent;
    color: #e0e0e0;
    font-size: 14px;
    font-family: inherit;
    outline: none;
  }

  .palette-input::placeholder {
    color: rgba(255, 255, 255, 0.3);
  }

  .palette-results {
    overflow-y: auto;
    flex: 1;
    padding: 6px 0;
  }

  .palette-empty {
    padding: 20px 14px;
    text-align: center;
    color: rgba(255, 255, 255, 0.3);
    font-size: 13px;
  }

  .palette-group {
    padding: 2px 0;
  }

  .palette-group-label {
    font-size: 10px;
    font-weight: 600;
    letter-spacing: 0.8px;
    color: rgba(255, 255, 255, 0.35);
    padding: 6px 14px 4px;
    text-transform: uppercase;
  }

  .palette-item {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 6px 14px;
    cursor: pointer;
    font-size: 13px;
    color: rgba(255, 255, 255, 0.8);
    transition: background 0.05s;
  }

  .palette-item.selected {
    background: rgba(255, 255, 255, 0.08);
  }

  .palette-item.disabled {
    color: rgba(255, 255, 255, 0.25);
    cursor: default;
  }

  .item-label {
    flex: 1;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .item-desc {
    font-size: 11px;
    color: rgba(255, 255, 255, 0.3);
    flex-shrink: 0;
  }

  .item-shortcut {
    font-size: 11px;
    color: rgba(255, 255, 255, 0.3);
    padding: 1px 6px;
    border-radius: 4px;
    background: rgba(255, 255, 255, 0.06);
    flex-shrink: 0;
  }
</style>
