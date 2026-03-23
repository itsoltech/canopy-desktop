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

  const DEFAULT_FONT_FAMILY = 'JetBrainsMono Nerd Font, FiraCode Nerd Font, Menlo, monospace'
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

  onMount(() => {
    let ws: WebSocket | null = null
    let resizeObserver: ResizeObserver | null = null

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

    ws = new WebSocket(wsUrl)
    ws.onmessage = (e): void => {
      term.write(e.data)
    }

    term.onData((data) => {
      if (ws && ws.readyState === WebSocket.OPEN) {
        ws.send(data)
      }
    })

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

    return () => {
      termRef = null
      if (resizeObserver) resizeObserver.disconnect()
      if (ws) ws.close()
      term.dispose()
    }
  })
</script>

<div class="terminal-container" bind:this={containerEl}></div>

<style>
  .terminal-container {
    width: 100%;
    height: 100%;
    background-color: var(--terminal-bg, #1e1e1e);
  }
</style>
