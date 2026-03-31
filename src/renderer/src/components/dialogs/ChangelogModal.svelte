<script lang="ts">
  import { onMount } from 'svelte'
  import { marked } from 'marked'
  import DOMPurify from 'dompurify'
  import { closeDialog } from '../../lib/stores/dialogs.svelte'

  let containerEl: HTMLDivElement | undefined = $state()

  interface Props {
    fromVersion: string
  }

  let { fromVersion }: Props = $props()

  let entries: Array<{ version: string; date: string; html: string }> = $state([])
  let loading = $state(true)
  let error = $state(false)

  onMount(async () => {
    containerEl?.focus()
    const raw = await window.api.getChangelogSinceVersion(fromVersion)
    if (raw && raw.length > 0) {
      entries = await Promise.all(
        raw.map(async (e) => ({
          version: e.version,
          date: e.date,
          html: DOMPurify.sanitize(await marked.parse(e.body)),
        })),
      )
    } else if (!raw) {
      error = true
    }
    loading = false
  })

  function handleKeydown(e: KeyboardEvent): void {
    if (e.key === 'Escape') {
      e.preventDefault()
      e.stopPropagation()
      closeDialog()
    }
  }

  function htmlContent(node: HTMLElement, content: () => string): void {
    $effect(() => {
      node.innerHTML = content()
    })
  }
</script>

<!-- svelte-ignore a11y_no_static_element_interactions -->
<div class="overlay" onkeydown={handleKeydown} onmousedown={closeDialog}>
  <!-- svelte-ignore a11y_click_events_have_key_events -->
  <div
    bind:this={containerEl}
    class="container"
    role="dialog"
    aria-modal="true"
    aria-labelledby="changelog-dialog-title"
    tabindex="-1"
    onmousedown={(e) => e.stopPropagation()}
  >
    <div class="header">
      <h2 id="changelog-dialog-title" class="title">What's New</h2>
      <span class="subtitle">Changes since v{fromVersion}</span>
    </div>

    <div class="content">
      {#if loading}
        <div class="loading">Loading release notes...</div>
      {:else if error}
        <div class="error-msg">Could not load release notes. Check your internet connection.</div>
      {:else if entries.length === 0}
        <div class="empty">No release notes found for this update.</div>
      {:else}
        {#each entries as entry, i (entry.version)}
          {#if i > 0}
            <div class="separator"></div>
          {/if}
          <div class="entry">
            <div class="entry-header">
              <span class="version-badge">v{entry.version}</span>
              <span class="date">{entry.date}</span>
            </div>
            <div
              class="entry-body"
              use:htmlContent={() => entry.html}
              onclick={(e: MouseEvent) => {
                const anchor = (e.target as HTMLElement).closest('a')
                if (anchor?.href) {
                  e.preventDefault()
                  window.api.openExternal(anchor.href)
                }
              }}
            ></div>
          </div>
        {/each}
      {/if}
    </div>

    <div class="actions">
      <button class="btn-close" onclick={closeDialog}>Close</button>
    </div>
  </div>
</div>

<style>
  .overlay {
    position: fixed;
    inset: 0;
    z-index: 1001;
    display: flex;
    justify-content: center;
    align-items: center;
    background: rgba(0, 0, 0, 0.5);
  }

  .container {
    outline: none;
    width: 560px;
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

  .header {
    text-align: center;
    margin-bottom: 16px;
    flex-shrink: 0;
  }

  .title {
    margin: 0;
    font-size: 20px;
    font-weight: 600;
    color: #e0e0e0;
    letter-spacing: 0.5px;
  }

  .subtitle {
    display: block;
    margin-top: 4px;
    font-size: 12px;
    color: rgba(255, 255, 255, 0.4);
  }

  .content {
    flex: 1;
    min-height: 0;
    overflow-y: auto;
    padding: 4px 0;
  }

  .loading,
  .error-msg,
  .empty {
    padding: 24px;
    text-align: center;
    font-size: 13px;
    color: rgba(255, 255, 255, 0.4);
  }

  .separator {
    height: 1px;
    background: rgba(255, 255, 255, 0.08);
    margin: 16px 0;
  }

  .entry-header {
    display: flex;
    align-items: center;
    gap: 8px;
    margin-bottom: 8px;
  }

  .version-badge {
    display: inline-block;
    padding: 2px 8px;
    background: rgba(116, 192, 252, 0.15);
    color: rgba(116, 192, 252, 0.9);
    border-radius: 4px;
    font-size: 12px;
    font-weight: 600;
    font-family: inherit;
  }

  .date {
    font-size: 12px;
    color: rgba(255, 255, 255, 0.3);
  }

  .entry-body {
    font-size: 13px;
    line-height: 1.6;
    color: rgba(255, 255, 255, 0.6);
  }

  .entry-body :global(h1),
  .entry-body :global(h2),
  .entry-body :global(h3) {
    margin: 10px 0 4px;
    font-size: 13px;
    font-weight: 600;
    color: rgba(255, 255, 255, 0.7);
  }

  .entry-body :global(h1) {
    font-size: 14px;
  }

  .entry-body :global(p) {
    margin: 0 0 6px;
  }

  .entry-body :global(ul),
  .entry-body :global(ol) {
    margin: 0 0 6px;
    padding-left: 20px;
  }

  .entry-body :global(li) {
    margin-bottom: 2px;
  }

  .entry-body :global(strong) {
    color: rgba(255, 255, 255, 0.7);
  }

  .entry-body :global(code) {
    padding: 1px 4px;
    background: rgba(255, 255, 255, 0.08);
    border-radius: 3px;
    font-size: 12px;
  }

  .entry-body :global(a) {
    color: rgba(116, 192, 252, 0.9);
    text-decoration: none;
  }

  .entry-body :global(a:hover) {
    text-decoration: underline;
  }

  .actions {
    display: flex;
    justify-content: center;
    margin-top: 16px;
    flex-shrink: 0;
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
