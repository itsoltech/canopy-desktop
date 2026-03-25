<script lang="ts">
  import { onMount } from 'svelte'
  import { marked } from 'marked'
  import { closeDialog } from '../../lib/stores/dialogs.svelte'

  let version = $state('')
  let homepage = $state('')
  let licenseHtml = $state('')

  onMount(async () => {
    const info = await window.api.getAboutInfo()
    version = info.version
    homepage = info.homepage
    licenseHtml = await marked.parse(info.license)
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
</script>

<!-- svelte-ignore a11y_no_static_element_interactions -->
<div class="about-overlay" onkeydown={handleKeydown} onclick={closeDialog}>
  <!-- svelte-ignore a11y_click_events_have_key_events -->
  <div class="about-container" onclick={(e) => e.stopPropagation()}>
    <div class="about-header">
      <h2 class="app-name">Canopy</h2>
      {#if version}
        <span class="app-version">Version {version}</span>
      {/if}
    </div>

    <p class="copyright">&copy; 2026 IT SOL Sp. z o.o.</p>

    {#if homepage}
      <button class="homepage-link" onclick={openHomepage}>
        {homepage.replace('https://', '')}
      </button>
    {/if}

    {#if licenseHtml}
      <div class="license-section">
        <h3 class="license-title">License</h3>
        <div class="license-content">
          <!-- eslint-disable-next-line svelte/no-at-html-tags -- trusted content from bundled LICENSE.md -->
          {@html licenseHtml}
        </div>
      </div>
    {/if}

    <div class="about-actions">
      <button class="btn btn-close" onclick={closeDialog}>Close</button>
    </div>
  </div>
</div>

<style>
  .about-overlay {
    position: fixed;
    inset: 0;
    z-index: 1001;
    display: flex;
    justify-content: center;
    align-items: center;
    background: rgba(0, 0, 0, 0.5);
  }

  .about-container {
    width: 400px;
    max-width: 90vw;
    max-height: 80vh;
    display: flex;
    flex-direction: column;
    background: rgba(30, 30, 30, 0.98);
    border: 1px solid rgba(255, 255, 255, 0.12);
    border-radius: 10px;
    box-shadow: 0 16px 48px rgba(0, 0, 0, 0.6);
    padding: 24px;
    overflow: hidden;
  }

  .about-header {
    text-align: center;
    margin-bottom: 16px;
  }

  .app-name {
    margin: 0;
    font-size: 20px;
    font-weight: 600;
    color: #e0e0e0;
    letter-spacing: 0.5px;
  }

  .app-version {
    display: block;
    margin-top: 4px;
    font-size: 12px;
    color: rgba(255, 255, 255, 0.4);
  }

  .copyright {
    margin: 0 0 12px;
    text-align: center;
    font-size: 13px;
    color: rgba(255, 255, 255, 0.5);
  }

  .homepage-link {
    display: block;
    margin: 0 auto 16px;
    padding: 0;
    border: none;
    background: transparent;
    font-size: 13px;
    font-family: inherit;
    color: rgba(116, 192, 252, 0.9);
    cursor: pointer;
    text-decoration: none;
  }

  .homepage-link:hover {
    text-decoration: underline;
  }

  .license-section {
    margin-bottom: 16px;
  }

  .license-title {
    margin: 0 0 8px;
    font-size: 13px;
    font-weight: 600;
    color: rgba(255, 255, 255, 0.6);
  }

  .license-content {
    max-height: 200px;
    overflow-y: auto;
    padding: 12px;
    background: rgba(0, 0, 0, 0.3);
    border-radius: 6px;
    border: 1px solid rgba(255, 255, 255, 0.06);
    font-size: 11px;
    line-height: 1.5;
    color: rgba(255, 255, 255, 0.5);
  }

  .license-content :global(h1),
  .license-content :global(h2),
  .license-content :global(h3) {
    margin: 12px 0 6px;
    font-size: 12px;
    font-weight: 600;
    color: rgba(255, 255, 255, 0.65);
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
    color: rgba(255, 255, 255, 0.6);
  }

  .about-actions {
    display: flex;
    justify-content: center;
  }

  .btn-close {
    padding: 6px 20px;
    border-radius: 6px;
    font-size: 13px;
    font-family: inherit;
    cursor: pointer;
    border: none;
    outline: none;
    background: rgba(255, 255, 255, 0.08);
    color: rgba(255, 255, 255, 0.7);
    transition: background 0.1s;
  }

  .btn-close:hover {
    background: rgba(255, 255, 255, 0.12);
  }

  .btn-close:focus-visible {
    outline: 2px solid rgba(116, 192, 252, 0.6);
    outline-offset: 1px;
  }
</style>
