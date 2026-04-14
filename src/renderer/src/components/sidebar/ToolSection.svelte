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
  <ul class="tool-list">
    {#each aiTools as tool (tool.id)}
      {@const profiles = getProfilesByAgent(tool.id as AgentType)}
      {@const count = runningCount(tool.id)}
      {@const isFlat = profiles.length <= 1}
      {@const defaultProfile = isFlat ? profiles[0] : undefined}
      {@const isOpen = expanded[tool.id]}
      <li>
        {#if isFlat}
          <!-- Single (or no) profile: render as a flat launcher, no accordion -->
          <button
            class="tool-item"
            onclick={() => onLaunchTool(tool.id, { profileId: defaultProfile?.id })}
            title={tool.name}
          >
            <ToolIcon icon={tool.icon} size={14} />
            <span class="tool-name">{tool.name}</span>
            {#if count > 0}
              <span class="badge">{count}</span>
            {/if}
          </button>
        {:else}
          <!-- Multiple profiles: expandable group -->
          <button
            class="tool-item"
            onclick={() => toggle(tool.id)}
            title={tool.name}
            aria-expanded={isOpen}
          >
            <span class="chevron" class:open={isOpen}>
              <ChevronRight size={10} />
            </span>
            <ToolIcon icon={tool.icon} size={14} />
            <span class="tool-name">{tool.name}</span>
            {#if count > 0}
              <span class="badge">{count}</span>
            {:else}
              <span class="badge subtle">{profiles.length}</span>
            {/if}
          </button>
          {#if isOpen}
            <ul class="profile-sublist">
              {#each profiles as p (p.id)}
                <li>
                  <button
                    class="profile-item"
                    onclick={() => onLaunchTool(tool.id, { profileId: p.id })}
                    title="Launch {tool.name} with {p.name}"
                  >
                    <span class="profile-dot"></span>
                    <span class="profile-name">{p.name}</span>
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
        <button class="tool-item" onclick={() => onLaunchTool(tool.id)} title={tool.name}>
          <ToolIcon icon={tool.icon} size={14} />
          <span class="tool-name">{tool.name}</span>
          {#if count > 0}
            <span class="badge">{count}</span>
          {/if}
        </button>
      </li>
    {/each}
  </ul>
</CollapsibleSection>

<style>
  .tool-list {
    list-style: none;
    padding: 0;
    margin: 0;
  }

  .tool-item {
    display: flex;
    align-items: center;
    gap: 8px;
    width: 100%;
    height: 28px;
    padding: 0 12px;
    border: none;
    background: none;
    color: var(--c-text);
    font-size: 12px;
    font-family: inherit;
    cursor: pointer;
    text-align: left;
    transition: background 0.1s;
  }

  .tool-item:hover {
    background: var(--c-hover);
  }

  .chevron {
    display: inline-flex;
    width: 10px;
    height: 10px;
    color: var(--c-text-faint);
    transition: transform 0.12s ease;
  }

  .chevron.open {
    transform: rotate(90deg);
  }

  .tool-name {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    flex: 1;
  }

  .badge {
    display: flex;
    align-items: center;
    justify-content: center;
    min-width: 16px;
    height: 16px;
    padding: 0 4px;
    border-radius: 8px;
    background: var(--c-border);
    color: var(--c-text);
    font-size: 10px;
    font-weight: 600;
    flex-shrink: 0;
  }

  .badge.subtle {
    background: transparent;
    color: var(--c-text-faint);
    font-weight: 500;
  }

  .profile-sublist {
    list-style: none;
    padding: 0;
    margin: 0 0 2px 0;
  }

  .profile-item {
    display: flex;
    align-items: center;
    gap: 8px;
    width: 100%;
    height: 24px;
    padding: 0 12px 0 30px;
    border: none;
    background: none;
    color: var(--c-text-secondary);
    font-size: 12px;
    font-family: inherit;
    cursor: pointer;
    text-align: left;
    transition: background 0.1s;
  }

  .profile-item:hover {
    background: var(--c-hover);
    color: var(--c-text);
  }

  .profile-dot {
    width: 4px;
    height: 4px;
    border-radius: 50%;
    background: var(--c-text-faint);
    flex-shrink: 0;
  }

  .profile-name {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    flex: 1;
  }
</style>
