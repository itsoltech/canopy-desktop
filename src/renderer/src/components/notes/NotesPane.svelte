<script lang="ts">
  import { marked } from 'marked'
  import DOMPurify from 'dompurify'
  import {
    notesState,
    notesUiScope,
    getNoteKey,
    getNoteLabel,
    type NoteScope,
  } from '../../lib/stores/notes.svelte'

  let { paneSessionId }: { paneSessionId: string } = $props()

  const scope = $derived<NoteScope>(notesUiScope[paneSessionId] ?? 'worktree')
  const key = $derived(getNoteKey(scope))
  const label = $derived(getNoteLabel(scope))
  const content = $derived(key ? (notesState[key] ?? '') : '')

  let previewHtml = $state('')
  let showPreview = $state(true)

  let parseGen = 0
  $effect(() => {
    const raw = content
    const gen = ++parseGen
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
    notesState[key] = target.value
  }

  // Assigns pre-sanitized HTML from `previewHtml`. Only call with DOMPurify-cleaned output.
  function htmlContent(node: HTMLElement, html: () => string): void {
    $effect(() => {
      node.innerHTML = html()
    })
  }
</script>

<div class="notes-pane">
  <header class="notes-header">
    <div class="scope-toggle" role="tablist" aria-label="Note scope">
      <button
        type="button"
        role="tab"
        id="notes-tab-worktree-{paneSessionId}"
        aria-controls="notes-panel-{paneSessionId}"
        aria-selected={scope === 'worktree'}
        class:active={scope === 'worktree'}
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
        class:active={scope === 'project'}
        onclick={() => setScope('project')}
      >
        Project
      </button>
    </div>
    <span class="scope-label" title={label}>{label}</span>
    <button
      type="button"
      class="preview-toggle"
      aria-pressed={showPreview}
      onclick={() => (showPreview = !showPreview)}
      title="Toggle preview"
    >
      {showPreview ? 'Hide preview' : 'Show preview'}
    </button>
  </header>

  {#if !key}
    <div class="empty-state">No active worktree.</div>
  {:else}
    <div
      class="notes-body"
      class:split={showPreview}
      role="tabpanel"
      id="notes-panel-{paneSessionId}"
      aria-labelledby="notes-tab-{scope}-{paneSessionId}"
    >
      <textarea
        class="editor"
        spellcheck="false"
        placeholder="# Notes — markdown supported. Lives only in memory (no file)."
        value={content}
        oninput={onInput}
      ></textarea>
      {#if showPreview}
        <div class="preview markdown-body" use:htmlContent={() => previewHtml}></div>
      {/if}
    </div>
  {/if}
</div>

<style>
  .notes-pane {
    display: flex;
    flex-direction: column;
    width: 100%;
    height: 100%;
    background: var(--bg-primary, #1e1e1e);
    color: var(--text-primary, #e0e0e0);
    font-family:
      system-ui,
      -apple-system,
      sans-serif;
  }

  .notes-header {
    display: flex;
    align-items: center;
    gap: 12px;
    padding: 6px 10px;
    border-bottom: 1px solid var(--border-color, #303030);
    background: var(--bg-secondary, #252525);
    font-size: 12px;
    flex-shrink: 0;
  }

  .scope-toggle {
    display: inline-flex;
    border: 1px solid var(--border-color, #3a3a3a);
    border-radius: 4px;
    overflow: hidden;
  }

  .scope-toggle button {
    background: transparent;
    border: 0;
    color: var(--text-secondary, #a0a0a0);
    padding: 3px 10px;
    font-size: 12px;
    cursor: pointer;
  }

  .scope-toggle button.active {
    background: var(--accent, #2563eb);
    color: white;
  }

  .scope-label {
    color: var(--text-secondary, #a0a0a0);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    flex: 1;
  }

  .preview-toggle {
    background: transparent;
    color: var(--text-secondary, #a0a0a0);
    border: 1px solid var(--border-color, #3a3a3a);
    border-radius: 4px;
    padding: 3px 8px;
    font-size: 12px;
    cursor: pointer;
  }

  .preview-toggle:hover {
    color: var(--text-primary, #e0e0e0);
  }

  .notes-body {
    flex: 1;
    display: grid;
    grid-template-columns: 1fr;
    min-height: 0;
  }

  .notes-body.split {
    grid-template-columns: 1fr 1fr;
  }

  .editor {
    background: var(--bg-primary, #1e1e1e);
    color: var(--text-primary, #e0e0e0);
    border: 0;
    border-right: 1px solid var(--border-color, #303030);
    outline: none;
    padding: 12px 14px;
    font-family: ui-monospace, 'SF Mono', Menlo, Consolas, monospace;
    font-size: 13px;
    line-height: 1.55;
    resize: none;
    width: 100%;
    height: 100%;
  }

  .notes-body:not(.split) .editor {
    border-right: 0;
  }

  .preview {
    overflow: auto;
    padding: 12px 16px;
    font-size: 13px;
    line-height: 1.55;
  }

  .preview :global(h1),
  .preview :global(h2),
  .preview :global(h3) {
    margin: 1em 0 0.4em;
  }

  .preview :global(code) {
    background: var(--bg-secondary, #2a2a2a);
    padding: 1px 4px;
    border-radius: 3px;
    font-family: ui-monospace, 'SF Mono', Menlo, monospace;
    font-size: 12px;
  }

  .preview :global(pre) {
    background: var(--bg-secondary, #2a2a2a);
    padding: 8px 10px;
    border-radius: 4px;
    overflow-x: auto;
  }

  .preview :global(pre code) {
    background: transparent;
    padding: 0;
  }

  .preview :global(a) {
    color: var(--accent, #60a5fa);
  }

  .empty-state {
    flex: 1;
    display: flex;
    align-items: center;
    justify-content: center;
    color: var(--text-secondary, #808080);
    font-size: 13px;
  }
</style>
