<script lang="ts">
  import { onMount } from 'svelte'
  import { Terminal } from '@xterm/xterm'
  import { FitAddon } from '@xterm/addon-fit'
  import { WebglAddon } from '@xterm/addon-webgl'
  import { LigaturesAddon } from '@xterm/addon-ligatures'
  import { ProgressAddon, type IProgressState } from '@xterm/addon-progress'
  import '@xterm/xterm/css/xterm.css'
  import { prefs } from '../stores/preferences.svelte'
  import { getTheme } from './themes'

  const DEFAULT_FONT_FAMILY =
    'JetBrains Mono, JetBrainsMono Nerd Font, JetBrainsMono NF, FiraCode Nerd Font, Fira Code, Menlo, monospace'
  const DEFAULT_FONT_SIZE = 13
  let {
    sessionId,
    wsUrl,
    active = true,
    onTitleChange,
  }: {
    sessionId: string
    wsUrl: string
    active?: boolean
    onTitleChange?: (title: string) => void
  } = $props()

  let containerEl: HTMLDivElement
  let termRef: Terminal | null = null
  let wsRef: WebSocket | null = null
  let webglAddonRef: WebglAddon | null = null
  let webglAttached = $state(true)
  let dragging = $state(false)
  let progressState = $state(0)
  let progressValue = $state(0)

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

  // Focus terminal when tab becomes active
  $effect(() => {
    if (active && termRef) {
      termRef.focus()
    }
  })

  // Re-attach WebGL addon after context loss when tab becomes active
  $effect(() => {
    if (active && termRef && !webglAttached) {
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
      const filePath = (file as File & { path?: string }).path
      if (filePath) paths.push(shellEscape(filePath))
    }

    if (paths.length > 0 && termRef) {
      termRef.paste(paths.join(' '))
    }
  }

  onMount(() => {
    let resizeObserver: ResizeObserver | null = null
    let disposed = false
    let reconnectAttempt = 0
    let reconnectTimer: ReturnType<typeof setTimeout> | null = null
    let dataDisposable: { dispose(): void } | null = null

    const MAX_RECONNECT_ATTEMPTS = 30
    const MAX_RECONNECT_DELAY = 8000

    function connectWs(term: Terminal): void {
      const ws = new WebSocket(wsUrl)
      wsRef = ws

      ws.onopen = (): void => {
        if (reconnectAttempt > 0) {
          term.write('\r\n\x1b[32m[reconnected]\x1b[0m\r\n')
        }
        reconnectAttempt = 0
      }

      ws.onmessage = (e): void => {
        term.write(e.data)
      }

      ws.onclose = (): void => {
        if (disposed) return
        scheduleReconnect(term)
      }

      ws.onerror = (): void => {
        // onclose fires after onerror, reconnect handled there
      }

      // Dispose previous data listener to avoid duplicates on reconnect
      if (dataDisposable) dataDisposable.dispose()
      dataDisposable = term.onData((data) => {
        if (wsRef && wsRef.readyState === WebSocket.OPEN) {
          wsRef.send(data)
        }
      })
    }

    function scheduleReconnect(term: Terminal): void {
      if (reconnectAttempt >= MAX_RECONNECT_ATTEMPTS) {
        term.write('\r\n\x1b[31m[disconnected]\x1b[0m\r\n')
        return
      }
      if (reconnectAttempt === 0) {
        term.write('\r\n\x1b[33m[reconnecting...]\x1b[0m\r\n')
      }
      const delay = Math.min(500 * Math.pow(2, reconnectAttempt), MAX_RECONNECT_DELAY)
      reconnectAttempt++
      reconnectTimer = setTimeout(() => {
        if (disposed) return
        connectWs(term)
      }, delay)
    }

    function initTerminal(): void {
      if (disposed) return

      const currentTheme = getTheme(prefs.theme || 'Default')
      const currentFontSize = parseInt(prefs.fontSize || '', 10) || DEFAULT_FONT_SIZE
      const currentFontFamily = prefs.fontFamily || DEFAULT_FONT_FAMILY

      const term = new Terminal({
        fontSize: currentFontSize,
        fontFamily: currentFontFamily,
        cursorBlink: true,
        allowProposedApi: true,
        theme: currentTheme,
      })

      const fitAddon = new FitAddon()
      const lightures = new LigaturesAddon()
      const progressAddon = new ProgressAddon()
      term.open(containerEl)
      term.loadAddon(progressAddon)
      term.loadAddon(fitAddon)
      term.loadAddon(lightures)

      progressAddon.onChange(({ state, value }: IProgressState) => {
        progressState = state
        progressValue = value
      })

      attachWebgl(term)

      // Defer initial fit to after browser layout is complete
      requestAnimationFrame(() => fitAddon.fit())
      termRef = term

      connectWs(term)

      term.attachCustomKeyEventHandler((event) => {
        if (event.type === 'keydown') {
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
          fitAddon.fit()
        }
      })
      resizeObserver.observe(containerEl)

      term.focus()
    }

    // Wait for system fonts to be available before initializing xterm.js
    document.fonts.ready.then(() => initTerminal())

    return () => {
      disposed = true
      if (reconnectTimer) clearTimeout(reconnectTimer)
      if (dataDisposable) dataDisposable.dispose()
      const term = termRef
      const ws = wsRef
      termRef = null
      wsRef = null
      if (resizeObserver) resizeObserver.disconnect()
      if (webglAddonRef) {
        webglAddonRef.dispose()
        webglAddonRef = null
        webglAttached = false
      }
      if (ws) ws.close()
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
    background-color: var(--terminal-bg, #1e1e1e);
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
