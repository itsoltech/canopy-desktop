<script lang="ts">
  import { FolderOpen, RotateCw } from '@lucide/svelte'

  let {
    filePath,
    active,
  }: {
    filePath: string
    active: boolean
  } = $props()

  let content: string | null = $state(null)
  let binary = $state(false)
  let truncated = $state(false)
  let fileSize = $state(0)
  let loading = $state(true)
  let error: string | null = $state(null)

  let fileName = $derived(filePath.split('/').pop() ?? filePath)
  function shortenPath(p: string): string {
    const parts = p.split('/')
    if (parts.length <= 4) return p
    return '.../' + parts.slice(-3).join('/')
  }

  let displayPath = $derived(shortenPath(filePath))

  let lineNumbers = $derived(
    content
      ? content
          .split('\n')
          .map((_l, i) => i + 1)
          .join('\n')
      : '',
  )

  async function loadFile(): Promise<void> {
    loading = true
    error = null
    content = null
    binary = false
    truncated = false
    try {
      const result = await window.api.readFile(filePath)
      if (result.binary) {
        binary = true
        fileSize = result.size
      } else {
        content = result.content
        truncated = result.truncated
        fileSize = result.size
      }
    } catch (e) {
      error = e instanceof Error ? e.message : 'Failed to read file'
    } finally {
      loading = false
    }
  }

  function formatSize(bytes: number): string {
    if (bytes < 1024) return `${bytes} B`
    if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`
    return `${(bytes / (1024 * 1024)).toFixed(1)} MB`
  }

  $effect(() => {
    void filePath
    loadFile()
  })
</script>

<div class="editor-pane" class:active>
  <div class="toolbar">
    <div class="file-info">
      <span class="file-name">{fileName}</span>
      <span class="file-path">{displayPath}</span>
      {#if fileSize > 0}
        <span class="file-size">{formatSize(fileSize)}</span>
      {/if}
    </div>
    <div class="toolbar-actions">
      <button class="toolbar-btn" onclick={loadFile} title="Refresh">
        <RotateCw size={13} />
      </button>
      <button
        class="toolbar-btn"
        onclick={() => window.api.showInFolder(filePath)}
        title="Show in Finder"
      >
        <FolderOpen size={13} />
      </button>
    </div>
  </div>

  <div class="content-area">
    {#if loading}
      <div class="status-message">Loading...</div>
    {:else if error}
      <div class="status-message error">{error}</div>
    {:else if binary}
      <div class="status-message">Binary file ({formatSize(fileSize)})</div>
    {:else if content !== null}
      <div class="code-view">
        <pre class="line-numbers">{lineNumbers}</pre>
        <pre class="code-content">{content}</pre>
      </div>
      {#if truncated}
        <div class="truncation-notice">
          File truncated at 1 MB (full size: {formatSize(fileSize)})
        </div>
      {/if}
    {/if}
  </div>
</div>

<style>
  .editor-pane {
    display: flex;
    flex-direction: column;
    width: 100%;
    height: 100%;
    background: var(--color-background);
  }

  .toolbar {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 4px 10px;
    height: 32px;
    min-height: 32px;
    background: rgba(30, 30, 30, 0.8);
    border-bottom: 1px solid rgba(255, 255, 255, 0.06);
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
    color: rgba(255, 255, 255, 0.85);
    white-space: nowrap;
  }

  .file-path {
    font-size: 11px;
    color: rgba(255, 255, 255, 0.35);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .file-size {
    font-size: 10px;
    color: rgba(255, 255, 255, 0.3);
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
    color: rgba(255, 255, 255, 0.4);
    cursor: pointer;
  }

  .toolbar-btn:hover {
    background: rgba(255, 255, 255, 0.08);
    color: rgba(255, 255, 255, 0.7);
  }

  .content-area {
    flex: 1;
    overflow: auto;
    position: relative;
  }

  .status-message {
    display: flex;
    align-items: center;
    justify-content: center;
    height: 100%;
    font-size: 13px;
    color: rgba(255, 255, 255, 0.4);
  }

  .status-message.error {
    color: #e05050;
  }

  .code-view {
    display: flex;
    min-height: 100%;
  }

  pre {
    margin: 0;
    font-family: 'SF Mono', 'Cascadia Code', 'Fira Code', 'JetBrains Mono', monospace;
    font-size: 12px;
    line-height: 1.5;
    tab-size: 4;
  }

  .line-numbers {
    position: sticky;
    left: 0;
    z-index: 1;
    padding: 8px 10px 8px 10px;
    text-align: right;
    color: rgba(255, 255, 255, 0.2);
    background: #1e1e1e;
    border-right: 1px solid rgba(255, 255, 255, 0.06);
    user-select: none;
    flex-shrink: 0;
  }

  .code-content {
    padding: 8px 16px;
    color: rgba(255, 255, 255, 0.8);
    white-space: pre;
    overflow-x: visible;
    flex: 1;
  }

  .truncation-notice {
    position: sticky;
    bottom: 0;
    left: 0;
    right: 0;
    padding: 6px 16px;
    font-size: 11px;
    color: rgba(255, 180, 80, 0.8);
    background: rgba(40, 35, 25, 0.95);
    border-top: 1px solid rgba(255, 180, 80, 0.2);
    text-align: center;
  }
</style>
