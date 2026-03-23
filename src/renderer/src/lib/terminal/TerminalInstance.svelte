<script lang="ts">
  import { onMount } from 'svelte'
  import { Terminal } from '@xterm/xterm'
  import { FitAddon } from '@xterm/addon-fit'
  import { WebglAddon } from '@xterm/addon-webgl'
  import { LigaturesAddon } from '@xterm/addon-ligatures'
  import { ProgressAddon, type IProgressState } from '@xterm/addon-progress'
  import '@xterm/xterm/css/xterm.css'

  let {
    sessionId,
    wsUrl,
    active = true
  }: {
    sessionId: string
    wsUrl: string
    active?: boolean
  } = $props()

  let containerEl: HTMLDivElement
  let termRef: Terminal | null = null

  // Focus terminal when tab becomes active
  $effect(() => {
    if (active && termRef) {
      termRef.focus()
    }
  })

  onMount(() => {
    let ws: WebSocket | null = null
    let resizeObserver: ResizeObserver | null = null

    const term = new Terminal({
      fontSize: 13,
      fontFamily: 'JetBrainsMono Nerd Font, FiraCode Nerd Font, Menlo, monospace',
      cursorBlink: true,
      allowProposedApi: true,
      theme: {
        background: '#1e1e1e',
        foreground: '#e0e0e0',
        cursor: '#ffffff',
        cursorAccent: '#000000',
        selectionBackground: 'rgba(255, 255, 255, 0.2)',
        black: '#1a1a1a',
        red: '#ff6b6b',
        green: '#69db7c',
        yellow: '#ffd43b',
        blue: '#74c0fc',
        magenta: '#da77f2',
        cyan: '#66d9e8',
        white: '#e0e0e0',
        brightBlack: '#686868',
        brightRed: '#ff8787',
        brightGreen: '#8ce99a',
        brightYellow: '#ffe066',
        brightBlue: '#a3d8f4',
        brightMagenta: '#e599f7',
        brightCyan: '#99e9f2',
        brightWhite: '#ffffff'
      }
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
    background-color: #1e1e1e;
  }
</style>
