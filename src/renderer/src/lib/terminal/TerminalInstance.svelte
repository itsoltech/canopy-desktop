<script lang="ts">
  import { onMount, untrack } from 'svelte'
  import { Terminal } from '@xterm/xterm'
  import { FitAddon } from '@xterm/addon-fit'
  import { WebglAddon } from '@xterm/addon-webgl'
  import { LigaturesAddon } from '@xterm/addon-ligatures'
  import { ProgressAddon, type IProgressState } from '@xterm/addon-progress'
  import { WebLinksAddon } from '@xterm/addon-web-links'
  import '@xterm/xterm/css/xterm.css'
  import { prefs } from '../stores/preferences.svelte'
  import { getTheme } from './themes'
  import { showUrlToast } from '../stores/toast.svelte'
  import { openTool } from '../stores/tabs.svelte'
  import { workspaceState } from '../stores/workspace.svelte'
  import { setConnectionStatus, clearConnectionStatus } from './connectionState.svelte'
  import { recordKeystroke, cleanupSession } from '../stores/wpmTracker.svelte'

  const DEFAULT_FONT_FAMILY =
    'JetBrains Mono, JetBrainsMono Nerd Font, JetBrainsMono NF, FiraCode Nerd Font, Fira Code, Menlo, monospace'
  const DEFAULT_FONT_SIZE = 13
  let {
    sessionId,
    wsUrl,
    active = true,
    visible = true,
    isAiTool = false,
    onTitleChange,
  }: {
    sessionId: string
    wsUrl: string
    active?: boolean
    visible?: boolean
    isAiTool?: boolean
    onTitleChange?: (title: string) => void
  } = $props()

  let containerEl: HTMLDivElement
  let termRef: Terminal | null = null
  let wsRef: WebSocket | null = null
  let webglAddonRef: WebglAddon | null = null
  let webglAttached = $state(false)
  let dragging = $state(false)
  let progressState = $state(0)
  let progressValue = $state(0)
  let disposed = false
  let reconnectAttempt = 0
  let reconnectTimer: ReturnType<typeof setTimeout> | null = null
  let dataDisposable: { dispose(): void } | null = null
  let resizeObserver: ResizeObserver | null = null
  let pendingData = ''
  let writeScheduled = false
  let writeRafId: number | null = null
  let receivedChars = 0
  let startTerminal: (() => void) | null = null

  const MAX_RECONNECT_ATTEMPTS = 30
  const MAX_RECONNECT_DELAY = 8000
  const MAX_PENDING_CHARS = 2 * 1024 * 1024 // ~2 MB for ASCII

  function attachWebgl(term: Terminal): void {
    if (webglAddonRef) return
    try {
      const addon = new WebglAddon()
      addon.onContextLoss(() => {
        addon.dispose()
        webglAddonRef = null
        webglAttached = false
      })
      term.loadAddon(addon)
      webglAddonRef = addon
      webglAttached = true
    } catch {
      webglAddonRef = null
      webglAttached = false
    }
  }

  // Focus terminal when tab becomes active (deferred to next frame so the
  // container has left display:none and the browser has finished layout)
  $effect(() => {
    if (active && termRef) {
      const term = termRef
      requestAnimationFrame(() => term.focus())
    }
  })

  // Listen for imperative focus requests (e.g. after browser screenshot delivery)
  $effect(() => {
    if (!termRef) return
    const term = termRef
    const handler = (e: Event): void => {
      const detail = (e as CustomEvent<{ sessionId: string }>).detail
      if (detail.sessionId === sessionId) {
        requestAnimationFrame(() => term.focus())
      }
    }
    window.addEventListener('canopy:focus-terminal', handler)
    return () => window.removeEventListener('canopy:focus-terminal', handler)
  })

  // Dispose WebGL addon when tab becomes inactive to free GPU memory
  $effect(() => {
    if (!active && webglAddonRef) {
      webglAddonRef.dispose()
      webglAddonRef = null
      webglAttached = false
    }
  })

  // Re-attach WebGL addon when tab becomes active (untrack webglAttached
  // so context-loss doesn't trigger an immediate retry loop)
  $effect(() => {
    if (active && termRef && !untrack(() => webglAttached)) {
      attachWebgl(termRef)
    }
  })

  // React to preference changes for theme/font
  $effect(() => {
    if (!termRef) return
    const themeName = prefs.theme || 'Default'
    const theme = getTheme(themeName)
    termRef.options.theme = theme
    if (containerEl) {
      containerEl.style.backgroundColor = theme.background ?? '#1e1e1e'
    }
  })

  $effect(() => {
    if (!termRef) return
    const size = parseInt(prefs.fontSize || '', 10) || DEFAULT_FONT_SIZE
    termRef.options.fontSize = size
  })

  $effect(() => {
    if (!termRef) return
    termRef.options.fontFamily = prefs.fontFamily || DEFAULT_FONT_FAMILY
  })

  function shellEscape(path: string): string {
    return "'" + path.replace(/'/g, "'\\''") + "'"
  }

  function scrollPreservingWrite(term: Terminal, data: string): void {
    const buffer = term.buffer.active
    const isAtBottom = buffer.viewportY >= buffer.baseY

    if (isAtBottom) {
      term.write(data)
    } else {
      const savedY = buffer.viewportY
      term.write(data, () => {
        const currentY = term.buffer.active.viewportY
        if (currentY !== savedY) {
          term.scrollLines(savedY - currentY)
        }
      })
    }
  }

  function disconnectWs(options: { suppressStatus?: boolean } = {}): void {
    if (reconnectTimer) {
      clearTimeout(reconnectTimer)
      reconnectTimer = null
    }
    reconnectAttempt = 0

    cancelPendingWrite()

    const ws = wsRef
    wsRef = null
    if (ws) {
      ws.onopen = null
      ws.onmessage = null
      ws.onclose = null
      ws.onerror = null
      ws.close()
    }

    if (!options.suppressStatus) {
      clearConnectionStatus(sessionId)
    }
  }

  function cancelPendingWrite(): void {
    if (writeRafId !== null) {
      cancelAnimationFrame(writeRafId)
      writeRafId = null
    }
    writeScheduled = false
  }

  /** Flush buffered data to the terminal and schedule a RAF write. */
  function flushPendingData(term: Terminal): void {
    if (!pendingData || writeScheduled) return
    writeScheduled = true
    writeRafId = requestAnimationFrame(() => {
      writeRafId = null
      const frameData = pendingData
      pendingData = ''
      writeScheduled = false
      scrollPreservingWrite(term, frameData)
    })
  }

  function scheduleReconnect(term: Terminal): void {
    if (reconnectAttempt >= MAX_RECONNECT_ATTEMPTS) {
      setConnectionStatus(sessionId, 'disconnected')
      return
    }
    if (reconnectAttempt === 0) {
      setConnectionStatus(sessionId, 'reconnecting')
    }
    const delay = Math.min(500 * Math.pow(2, reconnectAttempt), MAX_RECONNECT_DELAY)
    reconnectAttempt++
    reconnectTimer = setTimeout(() => {
      if (disposed) return
      connectWs(term)
    }, delay)
  }

  function connectWs(term: Terminal): void {
    if (disposed) return
    if (
      wsRef &&
      (wsRef.readyState === WebSocket.OPEN || wsRef.readyState === WebSocket.CONNECTING)
    ) {
      return
    }

    const url = new URL(wsUrl)
    url.searchParams.set('offset', String(receivedChars))
    const ws = new WebSocket(url)
    wsRef = ws

    ws.onopen = (): void => {
      if (reconnectAttempt > 0) {
        clearConnectionStatus(sessionId)
      }
      reconnectAttempt = 0
    }

    ws.onmessage = (e): void => {
      const chunk = typeof e.data === 'string' ? e.data : String(e.data)
      receivedChars += chunk.length
      pendingData += chunk
      if (pendingData.length > MAX_PENDING_CHARS) {
        pendingData = pendingData.slice(-MAX_PENDING_CHARS)
      }
      // Only schedule writes when the pane is visible
      if (visible && !writeScheduled) {
        writeScheduled = true
        writeRafId = requestAnimationFrame(() => {
          writeRafId = null
          const frameData = pendingData
          pendingData = ''
          writeScheduled = false
          scrollPreservingWrite(term, frameData)
        })
      }
    }

    ws.onclose = (): void => {
      if (disposed) return
      wsRef = null
      scheduleReconnect(term)
    }

    ws.onerror = (): void => {
      // onclose fires after onerror, reconnect handled there
    }
  }

  function handleDragOver(event: DragEvent): void {
    if (event.dataTransfer?.types.includes('Files')) {
      event.preventDefault()
      dragging = true
    }
  }

  function handleDragLeave(): void {
    dragging = false
  }

  function handleDrop(event: DragEvent): void {
    event.preventDefault()
    dragging = false

    const files = event.dataTransfer?.files
    if (!files || files.length === 0) return

    const paths: string[] = []
    for (const file of files) {
      const filePath = window.api.getPathForFile(file)
      if (filePath) paths.push(shellEscape(filePath))
    }

    if (paths.length > 0 && termRef) {
      termRef.paste(paths.join(' '))
      termRef.focus()
    }
  }

  $effect(() => {
    if (visible && !termRef) {
      startTerminal?.()
      return
    }
    if (!termRef) return
    if (visible) {
      // Becoming visible: ensure WS is connected and flush buffered output
      connectWs(termRef)
      flushPendingData(termRef)
    } else {
      // Becoming hidden: cancel pending RAF writes but keep WS connected
      // so background output is still buffered via receivedChars/pendingData
      cancelPendingWrite()
    }
  })

  onMount(() => {
    let fontsReady = false
    let initScheduled = false

    function initTerminal(): void {
      if (disposed) return

      const currentTheme = getTheme(prefs.theme || 'Default')
      const currentFontSize = parseInt(prefs.fontSize || '', 10) || DEFAULT_FONT_SIZE
      const currentFontFamily = prefs.fontFamily || DEFAULT_FONT_FAMILY

      // Set container bg before xterm renders to avoid flash of wrong color
      containerEl.style.backgroundColor = currentTheme.background ?? '#1e1e1e'

      const term = new Terminal({
        fontSize: currentFontSize,
        fontFamily: currentFontFamily,
        cursorBlink: true,
        allowProposedApi: true,
        theme: currentTheme,
        scrollback: 5000,
      })

      const fitAddon = new FitAddon()
      const lightures = new LigaturesAddon()
      const progressAddon = new ProgressAddon()
      term.open(containerEl)
      term.loadAddon(progressAddon)
      term.loadAddon(fitAddon)
      term.loadAddon(lightures)

      // URL detection — open based on urlOpenMode preference
      term.loadAddon(
        new WebLinksAddon((_event, url) => {
          const mode = prefs.urlOpenMode || 'ask'
          if (mode === 'canopy') {
            const path = workspaceState.selectedWorktreePath
            if (path) openTool('browser', path, url)
          } else if (mode === 'system') {
            window.api.openExternal(url)
          } else {
            showUrlToast(url)
          }
        }),
      )

      progressAddon.onChange(({ state, value }: IProgressState) => {
        progressState = state
        progressValue = value
      })

      if (active) attachWebgl(term)

      // Defer initial fit to after browser layout is complete
      requestAnimationFrame(() => fitAddon.fit())
      termRef = term

      dataDisposable = term.onData((data) => {
        if (wsRef && wsRef.readyState === WebSocket.OPEN) {
          wsRef.send(data)
        }
        recordKeystroke(sessionId, data)
      })

      if (visible) {
        connectWs(term)
      }

      const isMac = navigator.userAgent.includes('Mac')

      term.attachCustomKeyEventHandler((event) => {
        if (event.type === 'keydown') {
          // Ctrl+V → paste (Windows/Linux only; macOS uses Cmd+V natively)
          if (!isMac && event.ctrlKey && event.key === 'v') {
            return false
          }
          // Ctrl+C → copy selection (Windows/Linux only; let ^C through when no selection)
          if (!isMac && event.ctrlKey && event.key === 'c' && term.hasSelection()) {
            return false
          }
          // Shift+Enter → insert newline
          if (event.key === 'Enter' && event.shiftKey) {
            if (wsRef && wsRef.readyState === WebSocket.OPEN) {
              wsRef.send('\x1b\r')
            }
            return false
          }
          // Cmd+Backspace → kill line (delete to beginning)
          if (event.key === 'Backspace' && event.metaKey) {
            if (wsRef && wsRef.readyState === WebSocket.OPEN) {
              wsRef.send('\x15')
            }
            return false
          }
          // Block Ctrl+Z in AI tool terminals to prevent unrecoverable SIGTSTP
          if (isAiTool && event.ctrlKey && event.key === 'z') {
            return false
          }
        }
        return true
      })

      term.onResize(({ cols, rows }) => {
        window.api.resizePty(sessionId, cols, rows)
      })

      if (onTitleChange) {
        term.onTitleChange((title) => onTitleChange(title))
      }

      resizeObserver = new ResizeObserver(() => {
        if (!containerEl.clientWidth || !containerEl.clientHeight) return
        const dims = fitAddon.proposeDimensions()
        // Skip transient tiny sizes (e.g. window restore animation)
        if (!dims || dims.cols < 10 || dims.rows < 3) return
        if (dims.cols !== term.cols || dims.rows !== term.rows) {
          const buffer = term.buffer.active
          const isAtBottom = buffer.viewportY >= buffer.baseY
          const savedY = buffer.viewportY

          fitAddon.fit()

          if (!isAtBottom) {
            const currentY = term.buffer.active.viewportY
            if (currentY !== savedY) {
              term.scrollLines(savedY - currentY)
            }
          }
        }
      })
      resizeObserver.observe(containerEl)

      term.focus()
    }

    function maybeInitTerminal(): void {
      if (disposed || termRef || !visible || !fontsReady || initScheduled) return
      initScheduled = true

      requestAnimationFrame(() => {
        initScheduled = false
        if (disposed || termRef || !visible || !fontsReady) return
        if (!containerEl.clientWidth || !containerEl.clientHeight) {
          maybeInitTerminal()
          return
        }
        initTerminal()
      })
    }

    startTerminal = maybeInitTerminal

    // Wait for system fonts to be available before initializing xterm.js
    document.fonts.ready.then(() => {
      fontsReady = true
      maybeInitTerminal()
    })

    return () => {
      disposed = true
      startTerminal = null
      cleanupSession(sessionId)
      disconnectWs({ suppressStatus: true })
      if (dataDisposable) dataDisposable.dispose()
      const term = termRef
      termRef = null
      if (resizeObserver) resizeObserver.disconnect()
      if (webglAddonRef) {
        webglAddonRef.dispose()
        webglAddonRef = null
        webglAttached = false
      }
      clearConnectionStatus(sessionId)
      if (term) term.dispose()
    }
  })
</script>

<!-- svelte-ignore a11y_no_static_element_interactions -->
<div
  class="terminal-container"
  class:dragging
  bind:this={containerEl}
  ondragover={handleDragOver}
  ondragleave={handleDragLeave}
  ondrop={handleDrop}
>
  {#if progressState > 0}
    <div
      class="progress-bar"
      class:progress-error={progressState === 2}
      class:progress-indeterminate={progressState === 3}
      class:progress-warning={progressState === 4}
      style:width={progressState === 3 ? '100%' : `${progressValue}%`}
    ></div>
  {/if}
  {#if dragging}
    <div class="drop-overlay">Drop files</div>
  {/if}
</div>

<style>
  .terminal-container {
    width: 100%;
    height: 100%;
    padding: 8px;
    box-sizing: border-box;
    background-color: var(--c-bg, #1e1e1e);
    position: relative;
  }

  .progress-bar {
    position: absolute;
    top: 0;
    left: 0;
    height: 2px;
    background: #3b82f6;
    transition: width 0.3s ease;
    z-index: 5;
  }

  .progress-error {
    background: #ef4444;
  }

  .progress-warning {
    background: #eab308;
  }

  .progress-indeterminate {
    animation: indeterminate 1.5s ease-in-out infinite;
    background: linear-gradient(90deg, transparent, #3b82f6, transparent);
  }

  @keyframes indeterminate {
    0% {
      transform: translateX(-100%);
    }
    100% {
      transform: translateX(100%);
    }
  }

  @media (prefers-reduced-motion: reduce) {
    .progress-indeterminate {
      animation: none;
    }
  }

  .drop-overlay {
    position: absolute;
    inset: 0;
    z-index: 10;
    display: flex;
    align-items: center;
    justify-content: center;
    background: rgba(0, 0, 0, 0.5);
    border: 2px dashed rgba(116, 192, 252, 0.6);
    color: rgba(255, 255, 255, 0.8);
    font-size: 14px;
    pointer-events: none;
  }
</style>
