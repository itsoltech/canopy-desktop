<script lang="ts">
  import { onMount } from 'svelte'
  import { Restty, getBuiltinTheme } from 'restty'

  let containerEl: HTMLDivElement
  let restty: Restty | null = null
  let sessionId: string | null = null

  onMount(() => {
    let destroyed = false

    async function init(): Promise<void> {
      restty = new Restty({
        root: containerEl,
        appOptions: {
          onTermSize: (cols: number, rows: number) => {
            if (sessionId) {
              window.api.resizePty(sessionId, cols, rows)
            }
          }
        }
      })

      const theme = getBuiltinTheme('GruvboxDark')
      if (theme) restty.applyTheme(theme)

      if (destroyed) return

      const result = await window.api.spawnPty()
      if (destroyed) return

      sessionId = result.sessionId
      restty.connectPty(result.wsUrl)
      restty.focus()
    }

    init()

    return () => {
      destroyed = true
      if (restty) {
        restty.disconnectPty()
        restty.destroy()
      }
      if (sessionId) {
        window.api.killPty(sessionId)
      }
    }
  })
</script>

<div class="terminal-container" bind:this={containerEl}></div>

<style>
  .terminal-container {
    width: 100%;
    height: 100%;
  }
</style>
