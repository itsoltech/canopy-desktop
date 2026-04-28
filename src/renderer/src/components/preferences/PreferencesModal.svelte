<script lang="ts">
  import { onMount } from 'svelte'
  import { closeDialog } from '../../lib/stores/dialogs.svelte'
  import GeneralPrefs from './GeneralPrefs.svelte'
  import AppearancePrefs from './AppearancePrefs.svelte'
  import ToolPrefs from './ToolPrefs.svelte'
  import GitPrefs from './GitPrefs.svelte'
  import ShortcutsPrefs from './ShortcutsPrefs.svelte'
  import ClaudePrefs from './ClaudePrefs.svelte'
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

  let { section: initialSection }: { section?: string } = $props()

  let containerEl: HTMLDivElement | undefined = $state()

  const groups = [
    { label: 'General', sections: ['General', 'Updates', 'Privacy', 'Shortcuts'] },
    { label: 'Features', sections: ['Notch', 'Misc'] },
    { label: 'Appearance', sections: ['Appearance', 'Sidebar'] },
    { label: 'AI Agents', sections: ['Claude', 'Gemini', 'OpenCode', 'Codex', 'Skills'] },
    { label: 'Dev Tools', sections: ['Terminal', 'Tools', 'Git', 'Tasks', 'File Watcher'] },
    { label: 'Web Browser', sections: ['Web Browser'] },
    { label: 'Security', sections: ['Remote Control'] },
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
<div
  class="fixed inset-0 z-overlay flex justify-center items-center bg-scrim"
  onkeydown={handleKeydown}
  onmousedown={closeDialog}
>
  <!-- svelte-ignore a11y_click_events_have_key_events -->
  <div
    bind:this={containerEl}
    class="outline-none w-prefs max-w-prefs h-prefs max-h-prefs flex bg-bg-overlay border border-border rounded-xl shadow-modal overflow-hidden"
    role="dialog"
    aria-modal="true"
    aria-labelledby="prefs-dialog-title"
    tabindex="-1"
    onmousedown={(e) => e.stopPropagation()}
  >
    <div
      class="w-44 flex-shrink-0 border-r border-active py-3 flex flex-col overflow-y-auto select-none"
    >
      <h2
        id="prefs-dialog-title"
        class="text-2xs font-semibold uppercase tracking-caps-tight text-text-muted px-3 pb-2 m-0"
      >
        Settings
      </h2>
      {#each groups as group (group.label)}
        <div
          role="group"
          aria-labelledby={`prefs-group-${group.label}`}
          class="flex flex-col gap-px mt-2 first:mt-0"
        >
          <span
            id={`prefs-group-${group.label}`}
            class="block px-3 pb-1 text-2xs font-semibold text-text-faint uppercase tracking-caps-looser"
            >{group.label}</span
          >
          {#each group.sections as section (section)}
            <button
              class="block w-full px-5 py-1 border-0 bg-transparent text-text-secondary text-sm font-inherit text-left cursor-pointer rounded-sm transition-colors duration-fast hover:bg-hover hover:text-text"
              class:bg-active={activeSection === section}
              class:text-text={activeSection === section}
              onclick={() => (activeSection = section)}
            >
              {section}
            </button>
          {/each}
        </div>
      {/each}
    </div>

    <div class="flex-1 overflow-y-auto px-6 py-5">
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
      {/if}
    </div>
  </div>
</div>
