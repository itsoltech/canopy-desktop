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
  const AI_TOOL_IDS = new Set(['claude', 'codex', 'opencode', 'gemini'])

  let {
    sessionId,
    wsUrl,
    active = true,
    toolId,
    onTitleChange,
  }: {
    sessionId: string
    wsUrl: string
    active?: boolean
    toolId?: string
    onTitleChange?: (title: string) => void
  } = $props()

  let containerEl: HTMLDivElement
  let termRef: Terminal | null = null
  let wsRef: WebSocket | null = null
  let dragging = $state(false)

  // Focus terminal when tab becomes active
  $effect(() => {
    if (active && termRef) {
      termRef.focus()
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

    if (paths.length > 0 && wsRef && wsRef.readyState === WebSocket.OPEN) {
      wsRef.send(paths.join(' '))
    }
  }

  onMount(() => {
    let resizeObserver: ResizeObserver | null = null
    let disposed = false

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
        console.log(state, value)
      })

      try {
        term.loadAddon(new WebglAddon())
      } catch {
        // WebGL not available, fall back to canvas renderer
      }

      // Defer initial fit to after browser layout is complete
      requestAnimationFrame(() => fitAddon.fit())
      termRef = term

      const ws = new WebSocket(wsUrl)
      wsRef = ws
      ws.onmessage = (e): void => {
        term.write(e.data)
      }

      term.onData((data) => {
        if (ws && ws.readyState === WebSocket.OPEN) {
          ws.send(data)
        }
      })

      // Shift+Enter → CSI u newline for AI tools (kitty keyboard protocol)
      if (toolId && AI_TOOL_IDS.has(toolId)) {
        term.attachCustomKeyEventHandler((event) => {
          if (event.type === 'keydown' && event.key === 'Enter' && event.shiftKey) {
            if (ws && ws.readyState === WebSocket.OPEN) {
              ws.send('\x1b[13;2u')
            }
            return false
          }
          return true
        })
      }

      term.onResize(({ cols, rows }) => {
        window.api.resizePty(sessionId, cols, rows)
      })

      if (onTitleChange) {
        term.onTitleChange((title) => onTitleChange(title))
      }

      resizeObserver = new ResizeObserver(() => {
        // Skip when hidden (display:none gives 0 dimensions)
        if (!containerEl.clientWidth || !containerEl.clientHeight) return
        const dims = fitAddon.proposeDimensions()
        if (dims && (dims.cols !== term.cols || dims.rows !== term.rows)) {
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
      const term = termRef
      const ws = wsRef
      termRef = null
      wsRef = null
      if (resizeObserver) resizeObserver.disconnect()
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
  {#if dragging}
    <div class="drop-overlay">Drop files</div>
  {/if}
</div>

<style>
  .terminal-container {
    width: 100%;
    height: 100%;
    background-color: var(--terminal-bg, #1e1e1e);
    position: relative;
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
