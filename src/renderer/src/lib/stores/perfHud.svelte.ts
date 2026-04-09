// Reactive state + lifecycle for the status-bar perf HUD.
// The main process only samples while at least one renderer has subscribed,
// so disable() must always be called when the HUD toggle goes off.

export const perfHudState: { metrics: { cpu: number; memMb: number } | null } = $state({
  metrics: null,
})

let unsubMetrics: (() => void) | null = null
let active = false

export async function enablePerfHud(): Promise<void> {
  if (active) return
  active = true
  unsubMetrics = window.api.perfHud.onMetrics((m) => {
    perfHudState.metrics = m
  })
  await window.api.perfHud.start()
}

export async function disablePerfHud(): Promise<void> {
  if (!active) return
  active = false
  if (unsubMetrics) {
    unsubMetrics()
    unsubMetrics = null
  }
  perfHudState.metrics = null
  await window.api.perfHud.stop()
}
