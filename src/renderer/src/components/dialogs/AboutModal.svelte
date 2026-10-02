<script lang="ts">
  import { onMount, onDestroy } from 'svelte'
  import { closeDialog } from '../../lib/stores/dialogs.svelte'
  import Markdown from '../shared/Markdown.svelte'

  let containerEl: HTMLDivElement | undefined = $state()
  let version = $state('')
  let homepage = $state('')
  let licenseText = $state('')

  // Restore focus to the opener on close. Captured/restored via onDestroy rather
  // than a returned cleanup because an async onMount returns a promise, which
  // Svelte does not treat as a teardown function.
  let previouslyFocused: HTMLElement | null = null
  onDestroy(() => previouslyFocused?.focus?.())

  onMount(async () => {
    previouslyFocused = document.activeElement as HTMLElement | null
    containerEl?.focus()
    const info = await window.api.getAboutInfo()
    version = info.version
    homepage = info.homepage
    licenseText = info.license
  })

  function handleKeydown(e: KeyboardEvent): void {
    if (e.key === 'Escape') {
      e.preventDefault()
      e.stopPropagation()
      closeDialog()
      return
    }

    if (e.key === 'Tab' && containerEl) {
      const focusable = containerEl.querySelectorAll<HTMLElement>(
        'a[href], button:not([disabled]), input:not([disabled]), textarea:not([disabled]), select:not([disabled]), [tabindex]:not([tabindex="-1"])',
      )
      if (focusable.length === 0) return
      const first = focusable[0]
      const last = focusable[focusable.length - 1]
      const active = document.activeElement as HTMLElement | null
      if (e.shiftKey && (active === first || !containerEl.contains(active))) {
        e.preventDefault()
        last.focus()
      } else if (!e.shiftKey && active === last) {
        e.preventDefault()
        first.focus()
      }
    }
  }

  function openHomepage(): void {
    window.api.openExternal(homepage)
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
    class="outline-none w-100 max-w-dialog max-h-dialog-tall flex flex-col bg-bg-overlay border border-border rounded-2xl shadow-modal p-6 overflow-hidden"
    role="dialog"
    aria-modal="true"
    aria-labelledby="about-dialog-title"
    tabindex="-1"
    onmousedown={(e) => e.stopPropagation()}
  >
    <div class="text-center mb-4">
      <h2 id="about-dialog-title" class="m-0 text-2xl font-semibold text-text tracking-caps-tight">
        Canopy
      </h2>
      {#if version}
        <span class="block mt-1 text-sm text-text-muted">Version {version}</span>
      {/if}
    </div>

    <p class="m-0 mb-3 text-center text-md text-text-secondary">&copy; 2026 IT SOL Sp. z o.o.</p>

    {#if homepage}
      <button
        class="block mx-auto mb-4 p-0 border-0 bg-transparent text-md font-inherit text-accent-text cursor-pointer no-underline hover:underline"
        onclick={openHomepage}
      >
        {homepage.replace('https://', '')}
      </button>
    {/if}

    {#if licenseText}
      <div class="mb-4">
        <h3 class="m-0 mb-2 text-md font-semibold text-text-secondary">License</h3>
        <div
          class="license-content max-h-50 overflow-y-auto p-3 bg-bg-input rounded-lg border border-border-subtle text-xs leading-normal text-text-secondary"
        >
          <Markdown source={licenseText} />
        </div>
      </div>
    {/if}

    <div class="flex justify-center">
      <button
        class="px-5 py-1.5 rounded-lg text-md font-inherit cursor-pointer border-0 outline-none bg-active text-text transition-colors duration-fast hover:bg-border focus-visible:outline-2 focus-visible:outline-focus-ring focus-visible:outline-offset-1"
        onclick={closeDialog}>Close</button
      >
    </div>
  </div>
</div>
