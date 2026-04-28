<script lang="ts">
  import { ChevronRight } from 'lucide-svelte'
  import { workspaceState } from '../../lib/stores/workspace.svelte'
  import { getRunningCountByTool } from '../../lib/stores/tabs.svelte'
  import { getTools, getToolAvailability } from '../../lib/stores/tools.svelte'
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

  let availableTools = $derived(getTools().filter((t) => getToolAvailability()[t.id] !== false))
  let aiTools = $derived(availableTools.filter((t) => AI_TOOL_IDS.has(t.id)))
  let otherTools = $derived(availableTools.filter((t) => !AI_TOOL_IDS.has(t.id)))

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
    {#each aiTools as tool (tool.id)}
      {@const profiles = getProfilesByAgent(tool.id as AgentType)}
      {@const count = runningCount(tool.id)}
      {@const isFlat = profiles.length <= 1}
      {@const defaultProfile = isFlat ? profiles[0] : undefined}
      {@const isOpen = expanded[tool.id]}
      <li>
        {#if isFlat}
          <button
            class="flex items-center gap-2 w-full h-7 px-3 border-0 bg-transparent text-text text-sm font-inherit cursor-pointer text-left transition-colors duration-fast hover:bg-hover"
            onclick={() => onLaunchTool(tool.id, { profileId: defaultProfile?.id })}
            title={tool.name}
          >
            <ToolIcon icon={tool.icon} size={14} />
            <span class="overflow-hidden text-ellipsis whitespace-nowrap flex-1">{tool.name}</span>
            {#if count > 0}
              <span
                class="flex items-center justify-center min-w-4 h-4 px-1 rounded-2xl bg-border text-text text-2xs font-semibold flex-shrink-0"
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
            <span class="overflow-hidden text-ellipsis whitespace-nowrap flex-1">{tool.name}</span>
            {#if count > 0}
              <span
                class="flex items-center justify-center min-w-4 h-4 px-1 rounded-2xl bg-border text-text text-2xs font-semibold flex-shrink-0"
                >{count}</span
              >
            {:else}
              <span
                class="flex items-center justify-center min-w-4 h-4 px-1 rounded-2xl bg-transparent text-text-faint text-2xs font-medium flex-shrink-0"
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
      </li>
    {/each}
    {#each otherTools as tool (tool.id)}
      {@const count = runningCount(tool.id)}
      <li>
        <button
          class="flex items-center gap-2 w-full h-7 px-3 border-0 bg-transparent text-text text-sm font-inherit cursor-pointer text-left transition-colors duration-fast hover:bg-hover"
          onclick={() => onLaunchTool(tool.id)}
          title={tool.name}
        >
          <ToolIcon icon={tool.icon} size={14} />
          <span class="overflow-hidden text-ellipsis whitespace-nowrap flex-1">{tool.name}</span>
          {#if count > 0}
            <span
              class="flex items-center justify-center min-w-4 h-4 px-1 rounded-2xl bg-border text-text text-2xs font-semibold flex-shrink-0"
              >{count}</span
            >
          {/if}
        </button>
      </li>
    {/each}
  </ul>
</CollapsibleSection>
