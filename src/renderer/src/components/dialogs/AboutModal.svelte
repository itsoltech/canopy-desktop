<script lang="ts">
  import { onMount } from 'svelte'
  import { marked } from 'marked'
  import DOMPurify from 'dompurify'
  import { closeDialog } from '../../lib/stores/dialogs.svelte'

  let containerEl: HTMLDivElement | undefined = $state()
  let version = $state('')
  let homepage = $state('')
  let licenseHtml = $state('')

  onMount(async () => {
    containerEl?.focus()
    const info = await window.api.getAboutInfo()
    version = info.version
    homepage = info.homepage
    licenseHtml = DOMPurify.sanitize(await marked.parse(info.license))
  })

  function handleKeydown(e: KeyboardEvent): void {
    if (e.key === 'Escape') {
      e.preventDefault()
      e.stopPropagation()
      closeDialog()
    }
  }

  function openHomepage(): void {
    window.api.openExternal(homepage)
  }

  function htmlContent(node: HTMLElement, content: () => string): void {
    $effect(() => {
      node.innerHTML = content()
    })
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

    {#if licenseHtml}
      <div class="mb-4">
        <h3 class="m-0 mb-2 text-md font-semibold text-text-secondary">License</h3>
        <div
          class="license-content max-h-50 overflow-y-auto p-3 bg-bg-input rounded-lg border border-border-subtle text-xs leading-normal text-text-secondary"
        >
          <div use:htmlContent={() => licenseHtml}></div>
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

<!-- :global() rules style HTML rendered from sanitized markdown (license text) — required for cascade. -->
<style>
  .license-content :global(h1),
  .license-content :global(h2),
  .license-content :global(h3) {
    margin: 12px 0 6px;
    font-size: 12px;
    font-weight: 600;
    color: var(--color-text-secondary);
  }
  .license-content :global(h1) {
    font-size: 13px;
    margin-top: 0;
  }
  .license-content :global(p) {
    margin: 0 0 8px;
  }
  .license-content :global(ol),
  .license-content :global(ul) {
    margin: 0 0 8px;
    padding-left: 20px;
  }
  .license-content :global(li) {
    margin-bottom: 4px;
  }
  .license-content :global(strong) {
    color: var(--color-text-secondary);
  }
</style>
