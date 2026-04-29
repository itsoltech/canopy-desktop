<script lang="ts">
  import { onMount } from 'svelte'
  import { closeDialog } from '../../lib/stores/dialogs.svelte'
  import GeneralPrefs from './GeneralPrefs.svelte'
  import AppearancePrefs from './AppearancePrefs.svelte'
  import ToolPrefs from './ToolPrefs.svelte'
  import GitPrefs from './GitPrefs.svelte'
  import ShortcutsPrefs from './ShortcutsPrefs.svelte'
  import ClaudePrefs from './ClaudePrefs.svelte'
  import ClaudeSdkPrefs from './ClaudeSdkPrefs.svelte'
  import CodexSdkPrefs from './CodexSdkPrefs.svelte'
  import GeminiPrefs from './GeminiPrefs.svelte'
  import OpenCodePrefs from './OpenCodePrefs.svelte'
  import CodexPrefs from './CodexPrefs.svelte'
  import UpdatePrefs from './UpdatePrefs.svelte'
  import ViewportsPrefs from './ViewportsPrefs.svelte'
  import SidebarPrefs from './SidebarPrefs.svelte'
  import TerminalPrefs from './TerminalPrefs.svelte'
  import TaskTrackerPrefs from './TaskTrackerPrefs.svelte'
  import PrivacyPrefs from './PrivacyPrefs.svelte'
  import FileWatcherPrefs from './FileWatcherPrefs.svelte'
  import SkillPrefs from './SkillPrefs.svelte'
  import NotchPrefs from './NotchPrefs.svelte'
  import MiscPrefs from './MiscPrefs.svelte'
  import RemoteControlPrefs from './RemoteControlPrefs.svelte'
  import ExperimentalPrefs from './ExperimentalPrefs.svelte'

  let { section: initialSection }: { section?: string } = $props()

  let containerEl: HTMLDivElement | undefined = $state()

  const groups = [
    { label: 'General', sections: ['General', 'Updates', 'Privacy', 'Shortcuts'] },
    { label: 'Features', sections: ['Notch', 'Misc'] },
    { label: 'Appearance', sections: ['Appearance', 'Sidebar'] },
    {
      label: 'AI Agents',
      sections: ['Claude', 'Claude Agent', 'Codex Agent', 'Gemini', 'OpenCode', 'Codex', 'Skills'],
    },
    { label: 'Dev Tools', sections: ['Terminal', 'Tools', 'Git', 'Tasks', 'File Watcher'] },
    { label: 'Web Browser', sections: ['Web Browser'] },
    { label: 'Security', sections: ['Remote Control'] },
    { label: 'Experimental', sections: ['Experimental'] },
  ] as const

  type Section = (typeof groups)[number]['sections'][number]
  const allSections: readonly Section[] = groups.flatMap((g) => g.sections as readonly Section[])

  function resolveInitialSection(): Section {
    if (initialSection) {
      const match = allSections.find((s) => s.toLowerCase() === initialSection!.toLowerCase())
      if (match) return match
    }
    return 'General'
  }

  let activeSection: Section = $state(resolveInitialSection())

  onMount(() => {
    containerEl?.focus()
  })

  function handleKeydown(e: KeyboardEvent): void {
    if (e.key === 'Escape') {
      e.preventDefault()
      e.stopPropagation()
      closeDialog()
    }
  }
</script>

<!-- svelte-ignore a11y_no_static_element_interactions -->
<div class="prefs-overlay" onkeydown={handleKeydown} onmousedown={closeDialog}>
  <!-- svelte-ignore a11y_click_events_have_key_events -->
  <div
    bind:this={containerEl}
    class="prefs-container"
    role="dialog"
    aria-modal="true"
    aria-labelledby="prefs-dialog-title"
    tabindex="-1"
    onmousedown={(e) => e.stopPropagation()}
  >
    <div class="prefs-sidebar">
      <h2 id="prefs-dialog-title" class="prefs-title">Settings</h2>
      {#each groups as group (group.label)}
        <div role="group" aria-labelledby={`prefs-group-${group.label}`} class="prefs-group">
          <span id={`prefs-group-${group.label}`} class="prefs-group-label">{group.label}</span>
          {#each group.sections as section (section)}
            <button
              class="prefs-tab"
              class:active={activeSection === section}
              onclick={() => (activeSection = section)}
            >
              {section}
            </button>
          {/each}
        </div>
      {/each}
    </div>

    <div class="prefs-content">
      {#if activeSection === 'General'}
        <GeneralPrefs />
      {:else if activeSection === 'Updates'}
        <UpdatePrefs />
      {:else if activeSection === 'Appearance'}
        <AppearancePrefs />
      {:else if activeSection === 'Sidebar'}
        <SidebarPrefs />
      {:else if activeSection === 'Terminal'}
        <TerminalPrefs />
      {:else if activeSection === 'Tools'}
        <ToolPrefs />
      {:else if activeSection === 'Claude'}
        <ClaudePrefs />
      {:else if activeSection === 'Claude Agent'}
        <ClaudeSdkPrefs />
      {:else if activeSection === 'Codex Agent'}
        <CodexSdkPrefs />
      {:else if activeSection === 'Gemini'}
        <GeminiPrefs />
      {:else if activeSection === 'OpenCode'}
        <OpenCodePrefs />
      {:else if activeSection === 'Codex'}
        <CodexPrefs />
      {:else if activeSection === 'Skills'}
        <SkillPrefs />
      {:else if activeSection === 'Git'}
        <GitPrefs />
      {:else if activeSection === 'Web Browser'}
        <ViewportsPrefs />
      {:else if activeSection === 'Tasks'}
        <TaskTrackerPrefs />
      {:else if activeSection === 'File Watcher'}
        <FileWatcherPrefs />
      {:else if activeSection === 'Privacy'}
        <PrivacyPrefs />
      {:else if activeSection === 'Shortcuts'}
        <ShortcutsPrefs />
      {:else if activeSection === 'Notch'}
        <NotchPrefs />
      {:else if activeSection === 'Misc'}
        <MiscPrefs />
      {:else if activeSection === 'Remote Control'}
        <RemoteControlPrefs />
      {:else if activeSection === 'Experimental'}
        <ExperimentalPrefs />
      {/if}
    </div>
  </div>
</div>

<style>
  .prefs-overlay {
    position: fixed;
    inset: 0;
    z-index: 1001;
    display: flex;
    justify-content: center;
    align-items: center;
    background: var(--c-scrim);
  }

  .prefs-container {
    outline: none;
    width: 960px;
    max-width: 92vw;
    height: 680px;
    max-height: 88vh;
    display: flex;
    background: var(--c-bg-overlay);
    border: 1px solid var(--c-border);
    border-radius: 10px;
    box-shadow: var(--shadow-modal);
    overflow: hidden;
  }

  .prefs-sidebar {
    width: 150px;
    flex-shrink: 0;
    border-right: 1px solid var(--c-active);
    padding: 16px 0;
    display: flex;
    flex-direction: column;
    overflow-y: auto;
  }

  .prefs-title {
    font-size: 13px;
    font-weight: 600;
    color: var(--c-text-secondary);
    padding: 0 16px 12px;
    margin: 0;
    letter-spacing: 0.3px;
  }

  .prefs-group-label {
    display: block;
    padding: 8px 14px 4px;
    font-size: var(--fs-2xs);
    font-weight: 600;
    color: var(--c-text-secondary);
    text-transform: uppercase;
    letter-spacing: 0.6px;
    user-select: none;
  }

  .prefs-group:first-of-type > .prefs-group-label {
    padding-top: 0;
  }

  .prefs-group {
    display: flex;
    flex-direction: column;
    gap: 2px;
  }

  .prefs-tab {
    display: block;
    width: 100%;
    padding: 6px 14px 6px 26px;
    border: none;
    background: transparent;
    color: var(--c-text);
    font-size: 13px;
    font-family: inherit;
    text-align: left;
    cursor: pointer;
    transition: background var(--dur-fast);
  }

  .prefs-tab:hover {
    background: var(--c-hover);
  }

  .prefs-tab.active {
    background: var(--c-hover-strong);
    color: var(--c-text);
  }

  .prefs-content {
    flex: 1;
    padding: 20px 24px;
    overflow-y: auto;
  }
</style>
