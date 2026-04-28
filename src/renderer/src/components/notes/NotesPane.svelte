<script lang="ts">
  import { onDestroy } from 'svelte'
  import { marked } from 'marked'
  import DOMPurify from 'dompurify'
  import TurndownService from 'turndown'
  import {
    notesState,
    notesUiScope,
    getNoteKey,
    getNoteLabel,
    type NoteScope,
  } from '../../lib/stores/notes.svelte'

  const turndown = new TurndownService({
    headingStyle: 'atx',
    codeBlockStyle: 'fenced',
    bulletListMarker: '-',
  })

  let { paneSessionId }: { paneSessionId: string } = $props()

  const scope = $derived<NoteScope>(notesUiScope[paneSessionId] ?? 'worktree')
  const key = $derived(getNoteKey(scope))
  const label = $derived(getNoteLabel(scope))
  const content = $derived(key ? (notesState[key] ?? '') : '')

  let previewHtml = $state('')
  let showPreview = $state(true)
  let editSource: 'editor' | 'preview' | null = $state(null)
  let editSourceTimer: ReturnType<typeof setTimeout> | null = null
  let previewEl: HTMLDivElement | undefined = $state()

  onDestroy(() => {
    if (editSourceTimer) clearTimeout(editSourceTimer)
  })

  let parseGen = 0
  $effect(() => {
    const raw = content
    const gen = ++parseGen
    if (editSource === 'preview') return
    if (!raw.trim()) {
      previewHtml = ''
      return
    }
    Promise.resolve(marked.parse(raw)).then((html) => {
      // Drop stale results from earlier, slower parses.
      if (gen !== parseGen) return
      previewHtml = DOMPurify.sanitize(html)
    })
  })

  function setScope(next: NoteScope): void {
    notesUiScope[paneSessionId] = next
  }

  function onInput(e: Event): void {
    const target = e.target as HTMLTextAreaElement
    if (!key) return
    editSource = 'editor'
    if (editSourceTimer) clearTimeout(editSourceTimer)
    editSourceTimer = setTimeout(() => {
      editSource = null
    }, 350)
    notesState[key] = target.value
  }

  function onPreviewPaste(e: ClipboardEvent): void {
    e.preventDefault()
    const html = e.clipboardData?.getData('text/html') ?? ''
    const text = e.clipboardData?.getData('text/plain') ?? ''
    const sanitized = html ? DOMPurify.sanitize(html) : ''
    document.execCommand(sanitized ? 'insertHTML' : 'insertText', false, sanitized || text)
  }

  function onPreviewInput(): void {
    if (!previewEl || !key) return
    editSource = 'preview'
    if (editSourceTimer) clearTimeout(editSourceTimer)
    editSourceTimer = setTimeout(() => {
      editSource = null
    }, 350)
    const html = DOMPurify.sanitize(previewEl.innerHTML)
    const md = turndown.turndown(html)
    notesState[key] = md
  }

  // Assigns pre-sanitized HTML from `previewHtml`. Only call with DOMPurify-cleaned output.
  function htmlContent(node: HTMLElement, html: () => string): void {
    $effect(() => {
      const value = html()
      if (document.activeElement === node) return
      node.innerHTML = value
    })
  }
</script>

<div class="flex flex-col w-full h-full bg-bg text-text font-sans">
  <header
    class="flex items-center gap-3 px-2.5 py-1.5 border-b border-border bg-bg-elevated text-sm flex-shrink-0"
  >
    <div
      class="inline-flex border border-border rounded-md overflow-hidden"
      role="tablist"
      aria-label="Note scope"
    >
      <button
        type="button"
        role="tab"
        id="notes-tab-worktree-{paneSessionId}"
        aria-controls="notes-panel-{paneSessionId}"
        aria-selected={scope === 'worktree'}
        class="bg-transparent border-0 px-2.5 py-0.5 text-sm cursor-pointer"
        class:bg-accent={scope === 'worktree'}
        class:text-bg={scope === 'worktree'}
        class:text-text-secondary={scope !== 'worktree'}
        onclick={() => setScope('worktree')}
      >
        Worktree
      </button>
      <button
        type="button"
        role="tab"
        id="notes-tab-project-{paneSessionId}"
        aria-controls="notes-panel-{paneSessionId}"
        aria-selected={scope === 'project'}
        class="bg-transparent border-0 px-2.5 py-0.5 text-sm cursor-pointer"
        class:bg-accent={scope === 'project'}
        class:text-bg={scope === 'project'}
        class:text-text-secondary={scope !== 'project'}
        onclick={() => setScope('project')}
      >
        Project
      </button>
    </div>
    <span
      class="text-text-secondary overflow-hidden text-ellipsis whitespace-nowrap flex-1"
      title={label}>{label}</span
    >
    <button
      type="button"
      class="bg-transparent text-text-secondary border border-border rounded-md px-2 py-0.5 text-sm cursor-pointer hover:text-text"
      aria-pressed={showPreview}
      onclick={() => (showPreview = !showPreview)}
      title="Toggle preview"
    >
      {showPreview ? 'Hide preview' : 'Show preview'}
    </button>
  </header>

  {#if !key}
    <div class="flex-1 flex items-center justify-center text-text-muted text-md">
      No active worktree.
    </div>
  {:else}
    <div
      class="notes-body flex-1 grid min-h-0"
      class:grid-cols-1={!showPreview}
      class:grid-cols-2={showPreview}
      role="tabpanel"
      id="notes-panel-{paneSessionId}"
      aria-labelledby="notes-tab-{scope}-{paneSessionId}"
    >
      <textarea
        class="bg-bg text-text border-0 outline-none px-3.5 py-3 font-mono text-md leading-snug resize-none w-full h-full"
        class:border-r={showPreview}
        class:border-border={showPreview}
        spellcheck="false"
        placeholder="# Notes — markdown supported. Lives only in memory (no file)."
        value={content}
        oninput={onInput}
      ></textarea>
      {#if showPreview}
        <div
          class="markdown-body overflow-auto px-4 py-3 text-md leading-snug outline-none cursor-text empty:before:content-edit-hint empty:before:text-text-muted empty:before:italic"
          contenteditable="true"
          bind:this={previewEl}
          oninput={onPreviewInput}
          onpaste={onPreviewPaste}
          use:htmlContent={() => previewHtml}
        ></div>
      {/if}
    </div>
  {/if}
</div>

<!-- :global() rules style HTML rendered from markdown (sanitized DOMPurify output) which we
     don't author directly — required for the markdown preview cascade. -->
<style>
  .markdown-body :global(h1),
  .markdown-body :global(h2),
  .markdown-body :global(h3) {
    margin: 1em 0 0.4em;
  }

  .markdown-body :global(code) {
    background: var(--color-bg-elevated);
    padding: 1px 4px;
    border-radius: 3px;
    font-family: var(--font-mono);
    font-size: 12px;
  }

  .markdown-body :global(pre) {
    background: var(--color-bg-elevated);
    padding: 8px 10px;
    border-radius: 4px;
    overflow-x: auto;
  }

  .markdown-body :global(pre code) {
    background: transparent;
    padding: 0;
  }

  .markdown-body :global(a) {
    color: var(--color-accent);
  }
</style>
