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
<div
  class="fixed inset-0 z-[1001] flex justify-center items-center bg-scrim"
  onkeydown={handleKeydown}
  onmousedown={closeDialog}
>
  <!-- svelte-ignore a11y_click_events_have_key_events -->
  <div
    bind:this={containerEl}
    class="outline-none w-[560px] max-w-[90vw] max-h-[80vh] flex flex-col bg-bg-overlay border border-border rounded-[10px] shadow-modal p-6 overflow-hidden"
    role="dialog"
    aria-modal="true"
    aria-labelledby="changelog-dialog-title"
    tabindex="-1"
    onmousedown={(e) => e.stopPropagation()}
  >
    <div class="text-center mb-4 flex-shrink-0">
      <h2 id="changelog-dialog-title" class="m-0 text-2xl font-semibold text-text tracking-[0.5px]">
        What's New
      </h2>
      <span class="block mt-1 text-sm text-text-muted">Changes since v{fromVersion}</span>
    </div>

    <div class="flex-1 min-h-0 overflow-y-auto py-1">
      {#if loading}
        <div class="p-6 text-center text-md text-text-muted">Loading release notes...</div>
      {:else if error}
        <div class="p-6 text-center text-md text-text-muted">
          Could not load release notes. Check your internet connection.
        </div>
      {:else if entries.length === 0}
        <div class="p-6 text-center text-md text-text-muted">
          No release notes found for this update.
        </div>
      {:else}
        {#each entries as entry, i (entry.version)}
          {#if i > 0}
            <div class="h-px bg-active my-4"></div>
          {/if}
          <div>
            <div class="flex items-center gap-2 mb-2">
              <span
                class="inline-block px-2 py-0.5 bg-accent-bg text-accent-text rounded-md text-sm font-semibold font-inherit"
                >v{entry.version}</span
              >
              <span class="text-sm text-text-faint">{entry.date}</span>
            </div>
            <div
              class="entry-body text-md leading-[1.6] text-text-secondary"
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

    <div class="flex justify-center mt-4 flex-shrink-0">
      <button
        class="px-5 py-1.5 rounded-lg text-md font-inherit cursor-pointer border-0 outline-none bg-active text-text transition-colors duration-fast hover:bg-border focus-visible:outline-2 focus-visible:outline-focus-ring focus-visible:outline-offset-1"
        onclick={closeDialog}>Close</button
      >
    </div>
  </div>
</div>

<!-- :global() rules style HTML rendered from sanitized markdown — required for cascade. -->
<style>
  .entry-body :global(h1),
  .entry-body :global(h2),
  .entry-body :global(h3) {
    margin: 10px 0 4px;
    font-size: 13px;
    font-weight: 600;
    color: var(--color-text);
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
    color: var(--color-text);
  }
  .entry-body :global(code) {
    padding: 1px 4px;
    background: var(--color-active);
    border-radius: 3px;
    font-size: 12px;
  }
  .entry-body :global(a) {
    color: var(--color-accent-text);
    text-decoration: none;
  }
  .entry-body :global(a:hover) {
    text-decoration: underline;
  }
</style>
