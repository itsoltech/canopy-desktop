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
  import UpdatePrefs from './UpdatePrefs.svelte'
  import ViewportsPrefs from './ViewportsPrefs.svelte'
  import SidebarPrefs from './SidebarPrefs.svelte'
  import TerminalPrefs from './TerminalPrefs.svelte'

  let containerEl: HTMLDivElement | undefined = $state()

  const sections = [
    'General',
    'Updates',
    'Appearance',
    'Sidebar',
    'Terminal',
    'Tools',
    'Claude',
    'Gemini',
    'Git',
    'Web Browser',
    'Shortcuts',
  ] as const
  type Section = (typeof sections)[number]

  let activeSection: Section = $state('General')

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
      {#each sections as section (section)}
        <button
          class="prefs-tab"
          class:active={activeSection === section}
          onclick={() => (activeSection = section)}
        >
          {section}
        </button>
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
      {:else if activeSection === 'Gemini'}
        <GeminiPrefs />
      {:else if activeSection === 'Git'}
        <GitPrefs />
      {:else if activeSection === 'Web Browser'}
        <ViewportsPrefs />
      {:else if activeSection === 'Shortcuts'}
        <ShortcutsPrefs />
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
    width: 700px;
    max-width: 90vw;
    height: 500px;
    max-height: 80vh;
    display: flex;
    background: var(--c-bg-overlay);
    border: 1px solid var(--c-border);
    border-radius: 10px;
    box-shadow: 0 16px 48px rgba(0, 0, 0, 0.6);
    overflow: hidden;
  }

  .prefs-sidebar {
    width: 160px;
    flex-shrink: 0;
    border-right: 1px solid var(--c-active);
    padding: 16px 0;
    display: flex;
    flex-direction: column;
    gap: 2px;
  }

  .prefs-title {
    font-size: 13px;
    font-weight: 600;
    color: var(--c-text-secondary);
    padding: 0 16px 12px;
    margin: 0;
    letter-spacing: 0.3px;
  }

  .prefs-tab {
    display: block;
    width: 100%;
    padding: 6px 16px;
    border: none;
    background: transparent;
    color: var(--c-text);
    font-size: 13px;
    font-family: inherit;
    text-align: left;
    cursor: pointer;
    transition: background 0.1s;
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
