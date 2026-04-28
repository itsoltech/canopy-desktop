<script lang="ts">
  import { onMount, untrack } from 'svelte'
  import { Terminal } from '@xterm/xterm'
  import { FitAddon } from '@xterm/addon-fit'
  import { WebglAddon } from '@xterm/addon-webgl'
  import { LigaturesAddon } from '@xterm/addon-ligatures'
  import { ProgressAddon, type IProgressState } from '@xterm/addon-progress'
  import { WebLinksAddon } from '@xterm/addon-web-links'
  import '@xterm/xterm/css/xterm.css'
  import { prefs, getPref } from '../stores/preferences.svelte'
  import { getTheme } from './themes'
  import { showUrlToast } from '../stores/toast.svelte'
  import { openTool, openFile } from '../stores/tabs.svelte'
  import { detectPathsInText } from '../pathDetection/linkify'
  import { ensureLoaded, getFiles } from '../stores/quickOpenStore.svelte'
  import { workspaceState } from '../stores/workspace.svelte'
  import { setConnectionStatus, clearConnectionStatus } from './connectionState.svelte'
  import { recordKeystroke, cleanupSession } from '../stores/wpmTracker.svelte'
  import { recordKeyEvent, cleanupKeystrokeSession } from '../stores/keystrokeVisualizer.svelte'

  const DEFAULT_FONT_FAMILY =
    'JetBrains Mono, JetBrainsMono Nerd Font, JetBrainsMono NF, FiraCode Nerd Font, Fira Code, Menlo, monospace'
  const DEFAULT_FONT_SIZE = 13
  let {
    sessionId,
    wsUrl,
    active = true,
    focused = true,
    visible = true,
    isAiTool = false,
    onTitleChange,
  }: {
    sessionId: string
    wsUrl: string
    active?: boolean
    focused?: boolean
    visible?: boolean
    isAiTool?: boolean
    onTitleChange?: (title: string) => void
  } = $props()

  let containerEl: HTMLDivElement
  let termRef: Terminal | null = null
  let fitAddonRef: FitAddon | null = null
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
  let resizeDebounceTimer: ReturnType<typeof setTimeout> | null = null
  let receivedChars = 0
  let startTerminal: (() => void) | null = null

  const MAX_RECONNECT_ATTEMPTS = 30
  const MAX_RECONNECT_DELAY = 8000

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
    if (focused && termRef) {
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
      containerEl.style.backgroundColor = theme.background ?? 'var(--color-bg)'
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
      // Always write through to xterm regardless of visibility. Gating on
      // `visible` let pendingData accumulate while hidden and replay after
      // show; with a bounded cap the head got truncated and deltas were
      // applied to a stale buffer (overlapping / row-shifted TUI output).
      if (!writeScheduled) {
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
    // termRef is assigned inside initTerminal() and is not tracked by this
    // effect. initTerminal() handles the initial visible=true connection path.
    if (!termRef) return
    const term = termRef
    if (visible) {
      // Becoming visible: ensure WS is connected and flush any in-flight batch.
      connectWs(term)
      flushPendingData(term)
      // The container was `display:none` while hidden, so the xterm renderer
      // and ResizeObserver saw no meaningful dimensions. Now that layout is
      // back, refit if needed, force a full repaint to drop any stale canvas
      // state, and re-assert PTY size in case the window was resized while
      // this tab was hidden. Deferred a frame so the browser has finished
      // laying out the now-visible container.
      requestAnimationFrame(() => {
        if (disposed || !containerEl || !termRef) return
        const t = termRef
        const fit = fitAddonRef
        if (fit && containerEl.clientWidth && containerEl.clientHeight) {
          const dims = fit.proposeDimensions()
          if (
            dims &&
            dims.cols >= 10 &&
            dims.rows >= 3 &&
            (dims.cols !== t.cols || dims.rows !== t.rows)
          ) {
            const buffer = t.buffer.active
            const isAtBottom = buffer.viewportY >= buffer.baseY
            const savedY = buffer.viewportY
            fit.fit()
            if (!isAtBottom) {
              const currentY = t.buffer.active.viewportY
              if (currentY !== savedY) {
                t.scrollLines(savedY - currentY)
              }
            }
          }
        }
        t.refresh(0, t.rows - 1)
        if (t.cols > 0 && t.rows > 0) {
          window.api.resizePty(sessionId, t.cols, t.rows)
        }
      })
    }
  })

  onMount(() => {
    let fontsReady = false
    let initScheduled = false
    let keystrokeHandler: ((e: KeyboardEvent) => void) | null = null

    function initTerminal(): void {
      if (disposed) return

      const currentTheme = getTheme(prefs.theme || 'Default')
      const currentFontSize = parseInt(prefs.fontSize || '', 10) || DEFAULT_FONT_SIZE
      const currentFontFamily = prefs.fontFamily || DEFAULT_FONT_FAMILY

      // Set container bg before xterm renders to avoid flash of wrong color
      containerEl.style.backgroundColor = currentTheme.background ?? 'var(--color-bg)'

      const term = new Terminal({
        fontSize: currentFontSize,
        fontFamily: currentFontFamily,
        cursorBlink: true,
        allowProposedApi: true,
        theme: currentTheme,
        scrollback: 5000,
      })

      const fitAddon = new FitAddon()
      fitAddonRef = fitAddon
      const ligaturesAddon = new LigaturesAddon()
      const progressAddon = new ProgressAddon()
      term.open(containerEl)
      term.loadAddon(progressAddon)
      term.loadAddon(fitAddon)
      term.loadAddon(ligaturesAddon)

      // URL detection — open based on urlOpenMode preference
      term.loadAddon(
        new WebLinksAddon((_event, url) => {
          const mode = prefs.urlOpenMode || 'ask'
          if (mode === 'canopy') {
            const path = workspaceState.selectedWorktreePath
            if (path) openTool('browser', path, { initialUrl: url })
          } else if (mode === 'system') {
            window.api.openExternal(url)
          } else {
            showUrlToast(url)
          }
        }),
      )

      // File path detection — click opens file in editor. Only paths that
      // resolve to files tracked in the workspace are linkified, so arbitrary
      // strings with slashes in shell/agent output stay non-interactive.
      let knownFilesIdentity: string[] | null = null
      let knownFilesCache: Set<string> = new Set()
      function knownFilesFor(worktreePath: string): Set<string> {
        const list = getFiles(worktreePath)
        if (list.length === 0) {
          void ensureLoaded(worktreePath)
          return knownFilesCache
        }
        if (list !== knownFilesIdentity) {
          knownFilesIdentity = list
          knownFilesCache = new Set(list)
        }
        return knownFilesCache
      }

      term.registerLinkProvider({
        provideLinks: (bufferLineNumber, callback) => {
          const worktreePath = workspaceState.selectedWorktreePath
          if (!worktreePath) {
            callback(undefined)
            return
          }
          const buffer = term.buffer.active
          const lineEntry = buffer.getLine(bufferLineNumber - 1)
          if (!lineEntry) {
            callback(undefined)
            return
          }
          const lineText = lineEntry.translateToString(true)
          if (!lineText) {
            callback(undefined)
            return
          }
          const known = knownFilesFor(worktreePath)
          const matches = detectPathsInText(lineText, worktreePath, known)
          if (matches.length === 0) {
            callback(undefined)
            return
          }
          const links = matches.map((m) => ({
            range: {
              start: { x: m.start + 1, y: bufferLineNumber },
              end: { x: m.end, y: bufferLineNumber },
            },
            text: m.raw,
            activate: () => {
              openFile(m.absolutePath, worktreePath, { line: m.line })
            },
          }))
          callback(links)
        },
      })

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

      // Keystroke visualizer — capture keydown on container (avoids xterm API interference)
      if (getPref('keystrokeVisualizer.enabled') === 'true') {
        keystrokeHandler = (e: KeyboardEvent): void => {
          setTimeout(() => recordKeyEvent(sessionId, e), 0)
        }
        containerEl.addEventListener('keydown', keystrokeHandler, true)
      }

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
          // Shift+Enter → insert newline (same sequence as Option+Enter)
          // event.preventDefault() is required to suppress the keypress event that xterm.js
          // would otherwise fire (since returning false skips xterm's own preventDefault call)
          if (event.key === 'Enter' && event.shiftKey) {
            event.preventDefault()
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
        // Debounce: wait for resize to settle before re-fitting.
        // During continuous drag, this skips all intermediate fits,
        // avoiding WebGL texture churn (RAM spikes) and canvas flicker.
        if (resizeDebounceTimer !== null) clearTimeout(resizeDebounceTimer)
        resizeDebounceTimer = setTimeout(() => {
          resizeDebounceTimer = null
          if (!containerEl || !containerEl.clientWidth || !containerEl.clientHeight) return
          const dims = fitAddon.proposeDimensions()
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
        }, 80)
      })
      resizeObserver.observe(containerEl)

      // Focus/click re-claim: when a remote peer (phone, tablet) calls
      // `pty.resize` it shrinks the host PTY to its own viewport. If the
      // desktop user then clicks back into this terminal, we want the
      // PTY to immediately return to the desktop container's dimensions.
      //
      // We cannot rely on `fitAddon.fit()` doing this on its own: the
      // desktop xterm doesn't subscribe to `pty:resized` broadcasts, so
      // its local `term.cols/rows` remain at the desktop dimensions
      // even while the PTY is actually running at the peer's smaller
      // size. `fitAddon.proposeDimensions()` would return the desktop
      // dims, compare them against the local xterm state (which also
      // shows desktop dims), see no mismatch, and skip the resize —
      // leaving the PTY stuck at the peer's dimensions until a real
      // window resize fires the ResizeObserver.
      //
      // Instead we ALWAYS fire `window.api.resizePty` on focus/click
      // with the desktop xterm's current cols/rows. That lets the host
      // PTY snap back to the desktop layout on every interaction. The
      // IPC call is cheap and node-pty's internal resize is a no-op
      // when dims match, so there's no cost when nothing changed.
      const reclaimPty = (): void => {
        if (term.cols > 0 && term.rows > 0) {
          window.api.resizePty(sessionId, term.cols, term.rows)
        }
      }
      containerEl.addEventListener('pointerdown', reclaimPty)
      term.textarea?.addEventListener('focus', reclaimPty)

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
      cleanupKeystrokeSession(sessionId)
      if (keystrokeHandler) containerEl.removeEventListener('keydown', keystrokeHandler, true)
      disconnectWs({ suppressStatus: true })
      if (dataDisposable) dataDisposable.dispose()
      const term = termRef
      termRef = null
      fitAddonRef = null
      if (resizeDebounceTimer !== null) clearTimeout(resizeDebounceTimer)
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
  class="w-full h-full p-2 bg-bg relative overflow-hidden"
  ondragover={handleDragOver}
  ondragleave={handleDragLeave}
  ondrop={handleDrop}
>
  <div class="terminal-container w-full h-full" bind:this={containerEl}></div>
  {#if progressState > 0}
    <div
      class="absolute top-0 left-0 h-0.5 transition-all duration-slow ease-std z-pane-divider"
      class:bg-accent={progressState === 1}
      class:bg-danger={progressState === 2}
      class:bg-warning={progressState === 4}
      class:animate-progress-indeterminate={progressState === 3}
      class:motion-reduce:animate-none={progressState === 3}
      class:bg-progress-indeterminate={progressState === 3}
      style:width={progressState === 3 ? '100%' : `${progressValue}%`}
    ></div>
  {/if}
  {#if dragging}
    <div
      class="absolute inset-0 z-pane-overlay flex items-center justify-center bg-scrim border-2 border-dashed border-focus-ring text-text text-lg pointer-events-none"
    >
      Drop files
    </div>
  {/if}
</div>