<script lang="ts">
  import { onMount } from 'svelte'
  import { Download, Upload } from '@lucide/svelte'
  import { closeDialog, confirm } from '../../lib/stores/dialogs.svelte'
  import { loadPrefs } from '../../lib/stores/preferences.svelte'
  import { addToast } from '../../lib/stores/toast.svelte'
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
  import PrefsHeader from './_partials/PrefsHeader.svelte'
  import PrefsSidebar from './_partials/PrefsSidebar.svelte'
  import { sectionMeta } from './_partials/sectionMeta'
  import { clearQuery } from './_partials/prefsSearch.svelte'

  let { section: initialSection }: { section?: string } = $props()

  let containerEl: HTMLDivElement | undefined = $state()
  let searchInputEl: HTMLInputElement | undefined = $state()

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
  const activeMeta = $derived(sectionMeta[activeSection])

  function selectSection(section: string): void {
    activeSection = section as Section
  }

  function handleClose(): void {
    clearQuery()
    closeDialog()
  }

  onMount(() => {
    containerEl?.focus()
    return () => {
      clearQuery()
    }
  })

  function handleKeydown(e: KeyboardEvent): void {
    if (e.key === 'Escape') {
      e.preventDefault()
      e.stopPropagation()
      handleClose()
      return
    }
    if ((e.metaKey || e.ctrlKey) && e.key.toLowerCase() === 'k') {
      e.preventDefault()
      e.stopPropagation()
      searchInputEl?.focus()
      searchInputEl?.select()
    }
  }

  async function handleExport(): Promise<void> {
    const ok = await confirm({
      title: 'Export settings',
      message: 'Save all app settings and integrations to a JSON file?',
      details:
        'The file will contain your AI agent API keys, Linear/Jira tokens, and saved credentials as plaintext. Store it somewhere only you can access.',
      confirmLabel: 'Export',
    })
    if (!ok) return

    try {
      const result = await window.api.exportSettings()
      if (!result) return
      addToast(`Settings exported to ${result.path}`)
    } catch (e) {
      const message = e instanceof Error ? e.message : String(e)
      addToast(`Export failed: ${message}`)
    }
  }

  async function handleImport(): Promise<void> {
    const ok = await confirm({
      title: 'Import settings',
      message: 'Load settings from a JSON file?',
      details:
        'Existing settings with matching keys will be overwritten. Profiles, credentials, and custom tools not in the file are kept as-is.',
      confirmLabel: 'Import',
      destructive: true,
    })
    if (!ok) return

    try {
      const result = await window.api.importSettings()
      if (!result) return
      const { preferences, profiles, credentials, customTools } = result.counts
      addToast(
        `Imported ${preferences} preferences, ${profiles} profiles, ${credentials} credentials, ${customTools} tools`,
      )
      await loadPrefs()
    } catch (e) {
      const message = e instanceof Error ? e.message : String(e)
      addToast(`Import failed: ${message}`)
    }
  }
</script>

<!-- svelte-ignore a11y_no_static_element_interactions -->
<div
  class="fixed inset-0 z-overlay flex justify-center items-center bg-scrim"
  onkeydown={handleKeydown}
  onmousedown={handleClose}
>
  <!-- svelte-ignore a11y_click_events_have_key_events -->
  <div
    bind:this={containerEl}
    class="outline-none w-prefs max-w-prefs h-prefs max-h-prefs flex flex-col bg-bg-overlay border border-border rounded-xl shadow-modal overflow-hidden"
    role="dialog"
    aria-modal="true"
    aria-labelledby="prefs-dialog-title"
    tabindex="-1"
    onmousedown={(e) => e.stopPropagation()}
  >
    <PrefsHeader
      title="Settings"
      breadcrumb={activeSection}
      onclose={handleClose}
      bind:inputEl={searchInputEl}
    />

    <div class="flex flex-1 min-h-0">
      <PrefsSidebar {groups} {activeSection} onselect={selectSection}>
        {#snippet footer()}
          <div class="flex items-center justify-between gap-1">
            <span class="text-2xs uppercase tracking-caps-tight text-text-faint pl-1">Backup</span>
            <div class="flex items-center gap-0.5">
              <button
                type="button"
                class="flex items-center justify-center size-7 rounded-md bg-transparent border-0 text-text-muted cursor-pointer hover:bg-hover hover:text-text"
                aria-label="Export settings"
                title="Export settings…"
                onclick={handleExport}
              >
                <Download size={14} />
              </button>
              <button
                type="button"
                class="flex items-center justify-center size-7 rounded-md bg-transparent border-0 text-text-muted cursor-pointer hover:bg-hover hover:text-text"
                aria-label="Import settings"
                title="Import settings…"
                onclick={handleImport}
              >
                <Upload size={14} />
              </button>
            </div>
          </div>
        {/snippet}
      </PrefsSidebar>

      <main class="flex-1 flex flex-col min-w-0 overflow-hidden">
        <div class="px-7 pt-5 pb-3 border-b border-border-subtle shrink-0 flex flex-col gap-0.5">
          <h2 class="text-lg font-semibold text-text m-0 leading-tight">{activeSection}</h2>
          {#if activeMeta?.description}
            <p class="text-xs text-text-muted m-0 leading-snug">{activeMeta.description}</p>
          {/if}
        </div>

        <div class="flex-1 overflow-y-auto px-7 py-5">
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
      </main>
    </div>
  </div>
</div>
