<script lang="ts">
  import { FolderOpen, RotateCw, FileX, X, Save } from '@lucide/svelte'
  import { match, P } from 'ts-pattern'
  import CodeMirrorEditor from './CodeMirrorEditor.svelte'
  import {
    findEditorPane,
    setActiveEditorFile,
    updateEditorFileState,
    closeEditorFile,
    detachEditorFile,
    moveEditorFile,
    moveEditorFileBetweenPanes,
    mergeTabIntoEditorPane,
    pendingEditorJumps,
  } from '../../lib/stores/tabs.svelte'
  import { dragState, clearDrag, setDropTarget } from '../../lib/stores/dragState.svelte'
  import { detectIndent, indentUnitString, type IndentInfo } from './cm/detectIndent'
  import { detectLanguageName } from './cm/language'
  import { isStaleWriteError } from '../../lib/editor/fsErrors'

  let {
    paneId,
    active,
  }: {
    paneId: string
    active: boolean
  } = $props()

  const MAX_EDIT_BYTES = 2_097_152

  const pane = $derived(findEditorPane(paneId))
  const editorFiles = $derived(
    pane?.editorFiles ?? (pane?.filePath ? [{ filePath: pane.filePath }] : []),
  )
  const activeFilePath = $derived(
    pane?.editorActiveFile ?? pane?.filePath ?? editorFiles[0]?.filePath ?? '',
  )

  let content: string | null = $state(null)
  let editedContent = $state('')
  let binary = $state(false)
  let truncated = $state(false)
  let fileSize = $state(0)
  let canWrite = $state(false)
  let loading = $state(true)
  let error: string | null = $state(null)
  let saveError: string | null = $state(null)
  let fileDeleted = $state(false)
  let originalContent = $state('')
  let fileMtimeMs = $state(0)
  let fileLineEnding: 'LF' | 'CRLF' = $state('LF')
  let dirty = $state(false)
  let externalChangeDetected = $state(false)
  let editorRef:
    | {
        focus: () => void
        goToLine: (n: number) => void
        setContent: (v: string) => void
        setIndentUnit: (u: string) => void
        setLanguage: (filePath: string) => Promise<void>
        openBuffer: (filePath: string, doc: string, unit?: string) => void
        reloadBuffer: (doc: string, unit?: string) => void
        closeBuffer: (filePath: string) => void
      }
    | undefined = $state()
  let lastLoadedPath: string | null = null
  let skipNextWatcherEvent = false
  let indentInfo: IndentInfo = $state({ type: 'space', size: 4 })

  const languageName = $derived(activeFilePath ? detectLanguageName(activeFilePath) : null)

  // Sub-tab drag & drop state
  const DND_MIME = 'application/x-canopy-subtab'
  let draggingPath: string | null = $state(null)
  let dropIndex: number | null = $state(null)
  let externalDragOver = $state(false)
  let mainTabDragActive = $state(false)

  const fileName = $derived(activeFilePath.split('/').pop() ?? activeFilePath)
  function shortenPath(p: string): string {
    const parts = p.split('/')
    if (parts.length <= 4) return p
    return '.../' + parts.slice(-3).join('/')
  }
  const displayPath = $derived(shortenPath(activeFilePath))

  const canEdit = $derived(
    !binary && !truncated && fileSize <= MAX_EDIT_BYTES && canWrite && content !== null,
  )

  type ReadOnlyReason = 'binary' | 'too-large' | 'no-write' | null
  const readOnlyReason: ReadOnlyReason = $derived(
    match({ binary, truncated, oversized: fileSize > MAX_EDIT_BYTES, canWrite })
      .with({ binary: true }, () => 'binary' as const)
      .with(P.union({ truncated: true }, { oversized: true }), () => 'too-large' as const)
      .with({ canWrite: false }, () => 'no-write' as const)
      .otherwise(() => null),
  )

  function persistCurrentFileState(): void {
    if (!lastLoadedPath) return
    updateEditorFileState(paneId, lastLoadedPath, {
      dirty,
      originalContent,
      currentContent: editedContent,
      fileMtimeMs,
      fileLineEnding,
      externalChangeDetected,
    })
  }

  async function loadFile(path: string): Promise<void> {
    loading = true
    error = null
    saveError = null
    content = null
    binary = false
    truncated = false
    canWrite = false
    fileDeleted = false
    externalChangeDetected = false
    try {
      const readResult = await window.api.readFile(path, MAX_EDIT_BYTES)
      if (readResult.binary) {
        binary = true
        fileSize = readResult.size
      } else {
        content = readResult.content
        editedContent = readResult.content
        originalContent = readResult.content
        truncated = readResult.truncated
        fileSize = readResult.size
        fileLineEnding = readResult.content.includes('\r\n') ? 'CRLF' : 'LF'
        indentInfo = detectIndent(readResult.content)
        // Fresh disk content → drop any stale buffer for this file so we
        // never restore an out-of-date state, then open a new one.
        if (editorRef) {
          editorRef.closeBuffer(path)
          editorRef.openBuffer(path, readResult.content, indentUnitString(indentInfo))
        }
      }

      let statResult: { mtimeMs: number; size: number; canWrite: boolean } | null = null
      if (typeof window.api.statFile === 'function') {
        try {
          statResult = await window.api.statFile(path)
        } catch {
          statResult = null
        }
      }
      canWrite = statResult?.canWrite ?? true
      fileMtimeMs = statResult?.mtimeMs ?? 0
      dirty = false
      persistCurrentFileState()
    } catch (e) {
      error = e instanceof Error ? e.message : 'Failed to read file'
    } finally {
      loading = false
    }
  }

  function restoreFromState(state: {
    originalContent?: string
    currentContent?: string
    fileMtimeMs?: number
    fileLineEnding?: 'LF' | 'CRLF'
    dirty?: boolean
  }): void {
    const orig = state.originalContent ?? state.currentContent ?? ''
    const cur = state.currentContent ?? orig
    content = orig
    originalContent = orig
    editedContent = cur
    fileMtimeMs = state.fileMtimeMs ?? 0
    fileLineEnding = state.fileLineEnding ?? 'LF'
    dirty = state.dirty ?? cur !== orig
    fileSize = cur.length
    binary = false
    truncated = false
    canWrite = true
    loading = false
    error = null
    indentInfo = detectIndent(cur)
    // Restore: switch CM to the buffer keyed by this file path. If CM already
    // has a live state for it, its undo history comes back too. Otherwise a
    // fresh buffer is created with `cur` as the starting doc.
    editorRef?.openBuffer(activeFilePath, cur, indentUnitString(indentInfo))
  }

  function formatSize(bytes: number): string {
    if (bytes < 1024) return `${bytes} B`
    if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`
    return `${(bytes / (1024 * 1024)).toFixed(1)} MB`
  }

  function handleChange(value: string): void {
    editedContent = value
    const isDirty = value !== originalContent
    dirty = isDirty
    updateEditorFileState(paneId, activeFilePath, {
      dirty: isDirty,
      currentContent: value,
    })
  }

  async function handleSave(): Promise<void> {
    if (!dirty || !canEdit) return
    saveError = null
    const path = activeFilePath
    const contentToWrite =
      fileLineEnding === 'CRLF' ? editedContent.replace(/\r?\n/g, '\r\n') : editedContent
    skipNextWatcherEvent = true
    try {
      const result = await window.api.writeFile(path, contentToWrite, fileMtimeMs)
      originalContent = editedContent
      fileMtimeMs = result.mtimeMs
      fileSize = result.size
      dirty = false
      updateEditorFileState(paneId, path, {
        dirty: false,
        originalContent: editedContent,
        currentContent: editedContent,
        fileMtimeMs: result.mtimeMs,
        externalChangeDetected: false,
      })
      setTimeout(() => {
        skipNextWatcherEvent = false
      }, 1500)
    } catch (e) {
      skipNextWatcherEvent = false
      const message = e instanceof Error ? e.message : String(e)
      if (isStaleWriteError(e)) {
        externalChangeDetected = true
        updateEditorFileState(paneId, path, { externalChangeDetected: true })
        saveError = 'File changed on disk — reload before saving.'
      } else {
        saveError = `Failed to save: ${message}`
      }
    }
  }

  async function reloadAndDiscard(): Promise<void> {
    await loadFile(activeFilePath)
  }

  function keepMyChanges(): void {
    externalChangeDetected = false
    updateEditorFileState(paneId, activeFilePath, { externalChangeDetected: false })
  }

  function changeIndentType(type: 'space' | 'tab'): void {
    indentInfo = { ...indentInfo, type }
    editorRef?.setIndentUnit(indentUnitString(indentInfo))
  }
  function changeIndentSize(size: number): void {
    indentInfo = { ...indentInfo, size }
    editorRef?.setIndentUnit(indentUnitString(indentInfo))
  }

  function handleSubTabClick(path: string): void {
    if (path === activeFilePath) return
    persistCurrentFileState()
    setActiveEditorFile(paneId, path)
  }

  async function handleSubTabClose(evt: MouseEvent, path: string): Promise<void> {
    evt.stopPropagation()
    const state = editorFiles.find((f) => f.filePath === path)
    if (state?.dirty) {
      const choice = await window.api.confirmUnsavedChanges([path])
      if (choice === 'cancel') return
      if (choice === 'save') {
        try {
          const contentToWrite =
            state.fileLineEnding === 'CRLF'
              ? (state.currentContent ?? '').replace(/\r?\n/g, '\r\n')
              : (state.currentContent ?? '')
          skipNextWatcherEvent = true
          await window.api.writeFile(path, contentToWrite, state.fileMtimeMs)
          setTimeout(() => {
            skipNextWatcherEvent = false
          }, 1500)
        } catch (e) {
          skipNextWatcherEvent = false
          saveError = `Failed to save: ${e instanceof Error ? e.message : String(e)}`
          return
        }
      }
    }
    editorRef?.closeBuffer(path)
    closeEditorFile(paneId, path)
  }

  function handleSubTabDetach(evt: MouseEvent, path: string): void {
    evt.preventDefault()
    evt.stopPropagation()
    persistCurrentFileState()
    detachEditorFile(paneId, path)
  }

  function isSubTabDrag(evt: DragEvent): boolean {
    return evt.dataTransfer?.types?.includes(DND_MIME) ?? false
  }

  function handleSubTabDragStart(evt: DragEvent, path: string): void {
    if (!evt.dataTransfer) return
    evt.dataTransfer.effectAllowed = 'move'
    evt.dataTransfer.setData(DND_MIME, JSON.stringify({ sourcePaneId: paneId, filePath: path }))
    evt.dataTransfer.setData('text/plain', path)
    draggingPath = path
  }

  function handleSubTabDragOver(evt: DragEvent, index: number): void {
    if (!isSubTabDrag(evt)) return
    evt.preventDefault()
    evt.stopPropagation()
    if (evt.dataTransfer) evt.dataTransfer.dropEffect = 'move'
    // Only highlight the whole strip when drag comes from OUTSIDE this pane
    if (draggingPath === null) externalDragOver = true
    const target = evt.currentTarget as HTMLElement
    const rect = target.getBoundingClientRect()
    const midX = rect.left + rect.width / 2
    dropIndex = evt.clientX < midX ? index : index + 1
  }

  function handleStripDragOver(evt: DragEvent): void {
    if (!isSubTabDrag(evt)) return
    evt.preventDefault()
    if (evt.dataTransfer) evt.dataTransfer.dropEffect = 'move'
    if (draggingPath === null) externalDragOver = true
    if (dropIndex === null) dropIndex = editorFiles.length
  }

  function handleStripDrop(evt: DragEvent): void {
    if (!isSubTabDrag(evt)) return
    evt.preventDefault()
    const raw = evt.dataTransfer?.getData(DND_MIME)
    const target = dropIndex ?? editorFiles.length
    draggingPath = null
    dropIndex = null
    externalDragOver = false
    if (!raw) return
    try {
      const { sourcePaneId, filePath } = JSON.parse(raw) as {
        sourcePaneId: string
        filePath: string
      }
      if (sourcePaneId === paneId) {
        moveEditorFile(paneId, filePath, target)
      } else {
        moveEditorFileBetweenPanes(sourcePaneId, paneId, filePath, target)
      }
    } catch {
      // ignore malformed payload
    }
  }

  function handleStripDragLeave(evt: DragEvent): void {
    const strip = evt.currentTarget as HTMLElement
    const related = evt.relatedTarget as Node | null
    if (related && strip.contains(related)) return
    dropIndex = null
    externalDragOver = false
  }

  function isMainTabDrag(): boolean {
    return dragState.dragType === 'tab' && dragState.isDragging
  }

  function computeDropIndexFromStrip(strip: HTMLElement, clientX: number): number {
    const children = Array.from(strip.querySelectorAll('.sub-tab')) as HTMLElement[]
    for (let i = 0; i < children.length; i++) {
      const r = children[i].getBoundingClientRect()
      const mid = r.left + r.width / 2
      if (clientX < mid) return i
    }
    return children.length
  }

  function handleStripPointerEnter(evt: PointerEvent): void {
    if (!isMainTabDrag()) return
    mainTabDragActive = true
    // Prevent sibling pane-split logic from claiming this drag
    evt.stopPropagation()
    setDropTarget(null)
  }

  function handleStripPointerMove(evt: PointerEvent): void {
    if (!isMainTabDrag()) {
      if (mainTabDragActive) mainTabDragActive = false
      return
    }
    mainTabDragActive = true
    // Block PaneWrapper's window-level pointermove listener from assigning
    // a split drop zone while the pointer is over our strip.
    evt.stopPropagation()
    setDropTarget(null)
    const strip = evt.currentTarget as HTMLElement
    dropIndex = computeDropIndexFromStrip(strip, evt.clientX)
  }

  function handleStripPointerLeave(): void {
    if (mainTabDragActive) {
      mainTabDragActive = false
      dropIndex = null
    }
  }

  function handleStripPointerUp(): void {
    if (!mainTabDragActive || !isMainTabDrag()) {
      mainTabDragActive = false
      return
    }
    // Zero split-zone so TabBar's bubble-phase pointerup (which fires AFTER us)
    // sees dropTarget === null and skips the split. We intentionally let the
    // event propagate so TabBar can clean up its local dragActive/dragTabId.
    setDropTarget(null)
    const sourceTabId = dragState.sourceTabId
    const insertAt = dropIndex ?? editorFiles.length
    mainTabDragActive = false
    dropIndex = null
    if (sourceTabId) {
      mergeTabIntoEditorPane(sourceTabId, paneId, insertAt)
    }
    clearDrag()
  }

  function handleSubTabDragEnd(evt: DragEvent): void {
    const wasDragging = draggingPath
    const effect = evt.dataTransfer?.dropEffect
    draggingPath = null
    dropIndex = null
    externalDragOver = false
    if (wasDragging && effect === 'none' && editorFiles.length > 1) {
      persistCurrentFileState()
      detachEditorFile(paneId, wasDragging)
    }
  }

  // Load or restore whenever activeFilePath changes
  $effect(() => {
    const path = activeFilePath
    if (!path) return
    if (path === lastLoadedPath) return
    // Persist previous file's editing state before switching
    if (lastLoadedPath) {
      updateEditorFileState(paneId, lastLoadedPath, {
        dirty,
        originalContent,
        currentContent: editedContent,
        fileMtimeMs,
        fileLineEnding,
        externalChangeDetected,
      })
    }
    lastLoadedPath = path
    const existingState = editorFiles.find((f) => f.filePath === path)
    if (existingState?.currentContent !== undefined) {
      restoreFromState(existingState)
    } else {
      void loadFile(path)
    }
  })

  $effect(() => {
    const line = pendingEditorJumps[paneId]
    if (line && !loading && editorRef) {
      editorRef.goToLine(line)
      delete pendingEditorJumps[paneId]
    }
  })

  $effect(() => {
    const currentPath = activeFilePath
    const unsub = window.api.onFilesChanged((payload) => {
      for (const ev of payload.events) {
        const absPath = `${payload.repoRoot}/${ev.path}`
        if (absPath !== currentPath) continue
        if (ev.type === 'unlink') {
          fileDeleted = true
        } else if (ev.type === 'add' || ev.type === 'change') {
          fileDeleted = false
          if (skipNextWatcherEvent) {
            skipNextWatcherEvent = false
            continue
          }
          if (dirty) {
            externalChangeDetected = true
            updateEditorFileState(paneId, currentPath, { externalChangeDetected: true })
          } else {
            void loadFile(currentPath)
          }
        }
      }
    })
    return unsub
  })
</script>

<div class="editor-pane" class:active>
  {#if editorFiles.length > 0}
    <div
      class="sub-tabs"
      class:drop-target={externalDragOver || mainTabDragActive}
      ondragover={handleStripDragOver}
      ondrop={handleStripDrop}
      ondragleave={handleStripDragLeave}
      onpointerenter={handleStripPointerEnter}
      onpointermove={handleStripPointerMove}
      onpointerleave={handleStripPointerLeave}
      onpointerup={handleStripPointerUp}
      role="tablist"
    >
      {#each editorFiles as file, idx (file.filePath)}
        {@const isActive = file.filePath === activeFilePath}
        {@const name = file.filePath.split('/').pop() ?? file.filePath}
        {@const dragActive = draggingPath !== null || externalDragOver || mainTabDragActive}
        {@const showIndicatorBefore = dragActive && dropIndex === idx}
        {@const showIndicatorAfter =
          dragActive && dropIndex === idx + 1 && idx === editorFiles.length - 1}
        {#if showIndicatorBefore}<span class="sub-tab-drop-indicator"></span>{/if}
        <button
          class="sub-tab"
          class:active={isActive}
          class:dragging={draggingPath === file.filePath}
          draggable="true"
          ondragstart={(e) => handleSubTabDragStart(e, file.filePath)}
          ondragover={(e) => handleSubTabDragOver(e, idx)}
          ondragend={handleSubTabDragEnd}
          onmousedown={(e) => {
            if (e.button === 1) {
              e.preventDefault()
              e.stopPropagation()
              void handleSubTabClose(e, file.filePath)
            }
          }}
          onclick={() => handleSubTabClick(file.filePath)}
          oncontextmenu={(e) => handleSubTabDetach(e, file.filePath)}
          title={file.filePath + '\nMiddle-click: close · Drag to reorder · Drag out to new tab'}
        >
          {#if file.dirty}<span class="sub-tab-dirty" aria-label="Unsaved changes">●</span>{/if}
          <span class="sub-tab-name">{name}</span>
          <span
            class="sub-tab-close"
            role="button"
            tabindex="-1"
            aria-label="Close file"
            title="Close"
            onclick={(e) => handleSubTabClose(e, file.filePath)}
            onkeydown={(e) => {
              if (e.key === 'Enter' || e.key === ' ') {
                e.preventDefault()
                void handleSubTabClose(e as unknown as MouseEvent, file.filePath)
              }
            }}
          >
            <X size={11} />
          </span>
        </button>
        {#if showIndicatorAfter}<span class="sub-tab-drop-indicator"></span>{/if}
      {/each}
    </div>
  {/if}

  <div class="toolbar">
    <div class="file-info">
      {#if dirty}<span class="dirty-dot" aria-label="Unsaved changes">●</span>{/if}
      <span class="file-name">{fileName}</span>
      <span class="file-path">{displayPath}</span>
      {#if fileSize > 0}
        <span class="file-size">{formatSize(fileSize)}</span>
      {/if}
    </div>
    <div class="toolbar-actions">
      {#if canEdit}
        <button
          class="toolbar-btn"
          class:disabled={!dirty}
          onclick={handleSave}
          disabled={!dirty}
          title={dirty ? 'Save (Cmd/Ctrl+S)' : 'No changes'}
          aria-label="Save file"
        >
          <Save size={13} />
        </button>
      {/if}
      <button
        class="toolbar-btn"
        onclick={() => loadFile(activeFilePath)}
        title="Refresh"
        aria-label="Refresh file"
      >
        <RotateCw size={13} />
      </button>
      <button
        class="toolbar-btn"
        onclick={() => window.api.showInFolder(activeFilePath)}
        title="Show in Folder"
        aria-label="Show in Folder"
      >
        <FolderOpen size={13} />
      </button>
    </div>
  </div>

  {#if externalChangeDetected}
    <div class="conflict-banner">
      <span>File modified on disk</span>
      <div class="conflict-actions">
        <button class="conflict-btn" onclick={reloadAndDiscard}>Reload (discard changes)</button>
        <button class="conflict-btn primary" onclick={keepMyChanges}>Keep mine</button>
      </div>
    </div>
  {/if}

  {#if saveError}
    <div class="error-banner">
      <span>{saveError}</span>
      <button
        class="dismiss-btn"
        onclick={() => (saveError = null)}
        aria-label="Dismiss"
        title="Dismiss"
      >
        <X size={12} />
      </button>
    </div>
  {/if}

  {#if fileDeleted}
    <div class="deleted-banner">
      <FileX size={14} />
      <span>File deleted from disk</span>
      <button
        class="dismiss-btn"
        onclick={() => (fileDeleted = false)}
        aria-label="Dismiss notification"
        title="Dismiss"
      >
        <X size={12} />
      </button>
    </div>
  {/if}

  {#if readOnlyReason && !loading && !binary}
    <div class="readonly-banner">
      {#if readOnlyReason === 'too-large'}
        File too large to edit (> 2 MB) — read-only
      {:else if readOnlyReason === 'no-write'}
        No write permission — read-only
      {/if}
    </div>
  {/if}

  <div class="content-area">
    {#if loading}
      <div class="status-message">Loading...</div>
    {:else if error}
      <div class="status-message error">{error}</div>
    {:else if binary}
      <div class="status-message">Binary file ({formatSize(fileSize)})</div>
    {:else if canEdit}
      <CodeMirrorEditor
        bind:this={editorRef}
        initialValue={originalContent}
        initialIndentUnit={indentUnitString(indentInfo)}
        filePath={activeFilePath}
        onChange={handleChange}
        onSave={handleSave}
      />
    {:else if content !== null}
      <CodeMirrorEditor
        bind:this={editorRef}
        initialValue={originalContent}
        initialIndentUnit={indentUnitString(indentInfo)}
        filePath={activeFilePath}
        readOnly={true}
        onChange={handleChange}
      />
      {#if truncated}
        <div class="truncation-notice">
          File truncated at 2 MB (full size: {formatSize(fileSize)})
        </div>
      {/if}
    {/if}
  </div>

  {#if !loading && !error && !binary && content !== null}
    <div class="status-bar">
      <span class="status-item">{fileLineEnding}</span>
      {#if languageName}
        <span class="status-badge" title="Detected language">{languageName}</span>
      {/if}
      <div class="status-spacer"></div>
      <div class="status-group">
        <span class="status-label">Indent:</span>
        <button
          class="status-btn"
          class:active={indentInfo.type === 'space'}
          onclick={() => changeIndentType('space')}
          title="Use spaces">Spaces</button
        >
        <button
          class="status-btn"
          class:active={indentInfo.type === 'tab'}
          onclick={() => changeIndentType('tab')}
          title="Use tabs">Tabs</button
        >
      </div>
      {#if indentInfo.type === 'space'}
        <div class="status-group">
          <span class="status-label">Size:</span>
          {#each [2, 4, 8] as size (size)}
            <button
              class="status-btn"
              class:active={indentInfo.size === size}
              onclick={() => changeIndentSize(size)}
              title="{size} spaces">{size}</button
            >
          {/each}
        </div>
      {/if}
    </div>
  {/if}
</div>

<style>
  .editor-pane {
    display: flex;
    flex-direction: column;
    width: 100%;
    height: 100%;
    background: var(--c-bg);
  }

  .sub-tabs {
    display: flex;
    align-items: stretch;
    gap: 1px;
    background: var(--c-bg-glass-heavy);
    border-bottom: 1px solid var(--c-border-subtle);
    overflow-x: auto;
    min-height: 30px;
    flex-shrink: 0;
    user-select: none;
  }

  .sub-tab {
    display: flex;
    align-items: center;
    gap: 6px;
    padding: 4px 8px 4px 12px;
    max-width: 240px;
    background: transparent;
    border: none;
    border-right: 1px solid var(--c-border-subtle);
    color: var(--c-text-muted);
    font-size: 12px;
    cursor: pointer;
    white-space: nowrap;
    position: relative;
  }

  .sub-tab:hover {
    background: var(--c-hover);
    color: var(--c-text);
  }

  .sub-tab.active {
    background: var(--c-bg);
    color: var(--c-text);
  }

  .sub-tab.active::after {
    content: '';
    position: absolute;
    left: 0;
    right: 0;
    bottom: -1px;
    height: 2px;
    background: var(--c-accent);
  }

  .sub-tab-name {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .sub-tab-dirty {
    color: var(--c-text-muted);
    font-size: 9px;
    line-height: 1;
  }

  .sub-tab-close {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    width: 16px;
    height: 16px;
    border-radius: 3px;
    color: var(--c-text-muted);
    opacity: 0.6;
    cursor: pointer;
  }

  .sub-tab-close:hover {
    background: var(--c-hover-strong);
    opacity: 1;
  }

  .sub-tab.dragging {
    opacity: 0.4;
  }

  .sub-tabs.drop-target {
    background: var(--c-accent-bg, rgba(74, 158, 255, 0.12));
    box-shadow: inset 0 0 0 1px var(--c-accent, #4a9eff);
  }

  .sub-tab-drop-indicator {
    display: inline-block;
    width: 3px;
    align-self: stretch;
    background: var(--c-accent, #4a9eff);
    border-radius: 2px;
    flex-shrink: 0;
    box-shadow: 0 0 6px var(--c-accent, #4a9eff);
  }

  .toolbar {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 4px 10px;
    height: 32px;
    min-height: 32px;
    background: var(--c-bg-glass-heavy);
    border-bottom: 1px solid var(--c-border-subtle);
    user-select: none;
  }

  .file-info {
    display: flex;
    align-items: center;
    gap: 8px;
    min-width: 0;
    overflow: hidden;
  }

  .file-name {
    font-size: 12px;
    font-weight: 500;
    color: var(--c-text);
    white-space: nowrap;
  }

  .dirty-dot {
    color: var(--c-text-muted, #888);
    font-size: 12px;
    line-height: 1;
  }

  .file-path {
    font-size: 11px;
    color: var(--c-text-muted);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .file-size {
    font-size: 10px;
    color: var(--c-text-faint);
    white-space: nowrap;
  }

  .toolbar-actions {
    display: flex;
    gap: 2px;
    flex-shrink: 0;
  }

  .toolbar-btn {
    display: flex;
    align-items: center;
    justify-content: center;
    width: 24px;
    height: 24px;
    background: none;
    border: none;
    border-radius: 4px;
    color: var(--c-text-muted);
    cursor: pointer;
  }

  .toolbar-btn:hover {
    background: var(--c-active);
    color: var(--c-text);
  }

  .content-area {
    flex: 1;
    overflow: hidden;
    position: relative;
    min-height: 0;
    display: flex;
    flex-direction: column;
  }

  .status-message {
    display: flex;
    align-items: center;
    justify-content: center;
    height: 100%;
    font-size: 13px;
    color: var(--c-text-muted);
  }

  .status-message.error {
    color: var(--c-danger);
  }

  .truncation-notice {
    position: sticky;
    bottom: 0;
    left: 0;
    right: 0;
    padding: 6px 16px;
    font-size: 11px;
    color: var(--c-warning-text);
    background: var(--c-bg-overlay);
    border-top: 1px solid var(--c-border);
    text-align: center;
  }

  .deleted-banner,
  .readonly-banner {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 6px 12px;
    background: var(--c-warning-bg, var(--c-bg-overlay));
    border-bottom: 1px solid var(--c-border);
    color: var(--c-warning-text);
    font-size: 12px;
  }

  .error-banner {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 6px 12px;
    background: var(--c-error-bg, var(--c-bg-overlay));
    border-bottom: 1px solid var(--c-border);
    color: var(--c-error-text, var(--c-danger));
    font-size: 12px;
  }

  .conflict-banner {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 12px;
    padding: 6px 12px;
    background: var(--c-warning-bg, var(--c-bg-overlay));
    border-bottom: 1px solid var(--c-border);
    color: var(--c-warning-text);
    font-size: 12px;
  }

  .conflict-actions {
    display: flex;
    gap: 6px;
  }

  .conflict-btn {
    padding: 3px 10px;
    font-size: 11px;
    border-radius: 3px;
    border: 1px solid var(--c-border);
    background: var(--c-bg);
    color: var(--c-text);
    cursor: pointer;
  }

  .conflict-btn:hover {
    background: var(--c-hover);
  }

  .conflict-btn.primary {
    border-color: var(--c-accent, #4a9eff);
    background: var(--c-accent, #4a9eff);
    color: #fff;
  }

  .conflict-btn.primary:hover {
    filter: brightness(1.1);
  }

  .toolbar-btn.disabled,
  .toolbar-btn:disabled {
    opacity: 0.4;
    cursor: default;
  }

  .dismiss-btn {
    display: flex;
    align-items: center;
    justify-content: center;
    width: 18px;
    height: 18px;
    margin-left: auto;
    background: none;
    border: none;
    border-radius: 3px;
    color: var(--c-warning-text);
    cursor: pointer;
    opacity: 0.7;
    transition:
      background 0.1s,
      opacity 0.1s;
  }

  .dismiss-btn:hover {
    background: var(--c-hover);
    opacity: 1;
  }

  .status-bar {
    display: flex;
    align-items: center;
    gap: 12px;
    padding: 3px 10px;
    min-height: 24px;
    background: var(--c-bg-glass-heavy);
    border-top: 1px solid var(--c-border-subtle);
    font-size: 11px;
    color: var(--c-text-muted);
    user-select: none;
    flex-shrink: 0;
  }

  .status-spacer {
    flex: 1;
  }

  .status-item {
    font-variant-numeric: tabular-nums;
  }

  .status-badge {
    padding: 1px 6px;
    font-size: 10px;
    font-weight: 500;
    line-height: 1.4;
    border-radius: 3px;
    background: var(--c-accent-bg, rgba(74, 158, 255, 0.12));
    color: var(--c-accent-text, var(--c-accent, #4a9eff));
    border: 1px solid var(--c-accent-muted, rgba(74, 158, 255, 0.3));
    white-space: nowrap;
  }

  .status-group {
    display: flex;
    align-items: center;
    gap: 4px;
  }

  .status-label {
    color: var(--c-text-faint);
  }

  .status-btn {
    padding: 1px 6px;
    font-size: 11px;
    background: none;
    border: 1px solid transparent;
    border-radius: 3px;
    color: var(--c-text-muted);
    cursor: pointer;
    line-height: 1.4;
  }

  .status-btn:hover {
    background: var(--c-hover);
    color: var(--c-text);
  }

  .status-btn.active {
    background: var(--c-active);
    color: var(--c-text);
    border-color: var(--c-border);
  }
</style>
