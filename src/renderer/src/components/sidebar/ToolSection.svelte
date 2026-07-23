<script lang="ts">
  import { ChevronRight } from '@lucide/svelte'
  import { workspaceState } from '../../lib/stores/workspace.svelte'
  import { getRunningCountByTool } from '../../lib/stores/tabs.svelte'
  import { getTools, getToolAvailability, getToolsReady } from '../../lib/stores/tools.svelte'
  import { getToolView } from '../../lib/stores/toolView.svelte'
  import { getProfilesByAgent } from '../../lib/stores/profiles.svelte'
  import type { AgentType } from '../../../../main/agents/types'
  import ToolIcon from '../shared/ToolIcon.svelte'
  import CollapsibleSection from './CollapsibleSection.svelte'

  let {
    onLaunchTool,
  }: {
    onLaunchTool: (toolId: string, opts?: { profileId?: string }) => void
  } = $props()

  const AI_TOOL_IDS = new Set<string>(['claude', 'gemini', 'opencode', 'codex'])

  let toolsById = $derived(new Map(getTools().map((t) => [t.id, t])))
  // Tools the user has kept visible, in the configured order (before the
  // availability filter). AI and non-AI tools share one ordered list so the
  // user can sort them freely.
  let visibleTools = $derived(
    getToolView()
      .filter((e) => e.visible)
      .map((e) => toolsById.get(e.id))
      .filter((t): t is NonNullable<typeof t> => t !== undefined),
  )
  // Of the visible tools, only those currently available (installed /
  // resolvable) actually render as launchable rows.
  let orderedTools = $derived(visibleTools.filter((t) => getToolAvailability()[t.id] !== false))
  // Distinguish the empty-list causes so we never flash a false message: while
  // the tool store is still loading we stay silent; once ready, an empty list
  // is either "user hid everything" (no visible tools) or "the visible ones
  // aren't installed" (visible tools exist but none are available).
  let ready = $derived(getToolsReady())

  function loadExpanded(): Record<string, boolean> {
    const out: Record<string, boolean> = {}
    for (const id of AI_TOOL_IDS) {
      out[id] = localStorage.getItem(`canopy:toolgroup:${id}`) === '1'
    }
    return out
  }

  let expanded: Record<string, boolean> = $state(loadExpanded())

  function toggle(toolId: string): void {
    expanded[toolId] = !expanded[toolId]
    localStorage.setItem(`canopy:toolgroup:${toolId}`, expanded[toolId] ? '1' : '0')
  }

  function runningCount(toolId: string): number {
    const path = workspaceState.selectedWorktreePath
    if (!path) return 0
    return getRunningCountByTool(path, toolId)
  }
</script>

<CollapsibleSection title="TOOLS" sectionKey="tools" borderTop>
  <ul class="list-none p-0 m-0">
    {#if orderedTools.length > 0}
      {#each orderedTools as tool (tool.id)}
        {@const count = runningCount(tool.id)}
        <li>
          {#if AI_TOOL_IDS.has(tool.id)}
            {@const profiles = getProfilesByAgent(tool.id as AgentType)}
            {@const isFlat = profiles.length <= 1}
            {@const defaultProfile = isFlat ? profiles[0] : undefined}
            {@const isOpen = expanded[tool.id]}
            {#if isFlat}
              <button
                class="flex items-center gap-2 w-full h-7 px-3 border-0 bg-transparent text-text text-sm font-inherit cursor-pointer text-left transition-colors duration-fast hover:bg-hover"
                onclick={() => onLaunchTool(tool.id, { profileId: defaultProfile?.id })}
                title={tool.name}
              >
                <ToolIcon icon={tool.icon} size={14} />
                <span class="overflow-hidden text-ellipsis whitespace-nowrap flex-1"
                  >{tool.name}</span
                >
                {#if count > 0}
                  <span
                    class="inline-flex items-center justify-center min-w-4 h-4 px-1 rounded-sm bg-accent-bg text-accent-text text-2xs font-semibold tracking-caps-tight leading-tight flex-shrink-0"
                    >{count}</span
                  >
                {/if}
              </button>
            {:else}
              <button
                class="flex items-center gap-2 w-full h-7 px-3 border-0 bg-transparent text-text text-sm font-inherit cursor-pointer text-left transition-colors duration-fast hover:bg-hover"
                onclick={() => toggle(tool.id)}
                title={tool.name}
                aria-expanded={isOpen}
              >
                <span
                  class="inline-flex w-2.5 h-2.5 text-text-faint transition-transform duration-fast ease-std"
                  class:rotate-90={isOpen}
                >
                  <ChevronRight size={10} />
                </span>
                <ToolIcon icon={tool.icon} size={14} />
                <span class="overflow-hidden text-ellipsis whitespace-nowrap flex-1"
                  >{tool.name}</span
                >
                {#if count > 0}
                  <span
                    class="inline-flex items-center justify-center min-w-4 h-4 px-1 rounded-sm bg-accent-bg text-accent-text text-2xs font-semibold tracking-caps-tight leading-tight flex-shrink-0"
                    >{count}</span
                  >
                {:else}
                  <span
                    class="inline-flex items-center justify-center min-w-4 h-4 px-1 rounded-sm bg-border-subtle text-text-muted text-2xs font-semibold tracking-caps-tight leading-tight flex-shrink-0"
                    >{profiles.length}</span
                  >
                {/if}
              </button>
              {#if isOpen}
                <ul class="list-none p-0 m-0 mb-0.5">
                  {#each profiles as p (p.id)}
                    <li>
                      <button
                        class="flex items-center gap-2 w-full h-6 pr-3 pl-7 border-0 bg-transparent text-text-secondary text-sm font-inherit cursor-pointer text-left transition-colors duration-fast hover:bg-hover hover:text-text"
                        onclick={() => onLaunchTool(tool.id, { profileId: p.id })}
                        title="Launch {tool.name} with {p.name}"
                      >
                        <span class="w-1 h-1 rounded-full bg-text-faint flex-shrink-0"></span>
                        <span class="overflow-hidden text-ellipsis whitespace-nowrap flex-1"
                          >{p.name}</span
                        >
                      </button>
                    </li>
                  {/each}
                </ul>
              {/if}
            {/if}
          {:else}
            <button
              class="flex items-center gap-2 w-full h-7 px-3 border-0 bg-transparent text-text text-sm font-inherit cursor-pointer text-left transition-colors duration-fast hover:bg-hover"
              onclick={() => onLaunchTool(tool.id)}
              title={tool.name}
            >
              <ToolIcon icon={tool.icon} size={14} />
              <span class="overflow-hidden text-ellipsis whitespace-nowrap flex-1">{tool.name}</span
              >
              {#if count > 0}
                <span
                  class="inline-flex items-center justify-center min-w-4 h-4 px-1 rounded-sm bg-accent-bg text-accent-text text-2xs font-semibold tracking-caps-tight leading-tight flex-shrink-0"
                  >{count}</span
                >
              {/if}
            </button>
          {/if}
        </li>
      {/each}
    {:else if !ready}
      <!-- Tool store still loading: render nothing rather than flash a false
           "all hidden" / "none available" message before prefs and availability
           resolve. -->
    {:else if visibleTools.length === 0}
      <li class="px-3 py-1.5 text-sm text-text-faint">
        All tools hidden — enable them in Settings → Tools.
      </li>
    {:else}
      <li class="px-3 py-1.5 text-sm text-text-faint">
        No visible tools are available — check they're installed, or enable others in Settings →
        Tools.
      </li>
    {/if}
  </ul>
</CollapsibleSection>
