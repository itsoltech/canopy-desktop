<script lang="ts">
  import { onMount } from 'svelte'
  import { init, Terminal, FitAddon } from 'ghostty-web'

  let { sessionId, wsUrl }: { sessionId: string; wsUrl: string } = $props()

  let containerEl: HTMLDivElement

  onMount(() => {
    let term: Terminal | null = null
    let ws: WebSocket | null = null

    async function setup(): Promise<void> {
      await init()

      term = new Terminal({
        fontSize: 13,
        fontFamily: 'JetBrainsMono Nerd Font, FiraCode Nerd Font, Menlo, monospace',
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
      term.loadAddon(fitAddon)
      term.open(containerEl)
      fitAddon.fit()

      ws = new WebSocket(wsUrl)
      ws.onmessage = (e): void => {
        term!.write(e.data)
      }

      term.onData((data) => {
        if (ws && ws.readyState === WebSocket.OPEN) {
          ws.send(data)
        }
      })

      term.onResize(({ cols, rows }) => {
        window.api.resizePty(sessionId, cols, rows)
      })

      const resizeObserver = new ResizeObserver(() => fitAddon.fit())
      resizeObserver.observe(containerEl)

      term.focus()
    }

    setup()

    return () => {
      if (ws) ws.close()
      if (term) term.dispose()
      // Do NOT kill PTY here — the sessions store manages lifecycle
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
