// Small deterministic time formatters for the CI activity views (pure — `now` is a
// parameter so tests don't depend on the wall clock).

export function formatDuration(ms: number): string {
  const totalSeconds = Math.floor(ms / 1000)
  if (totalSeconds < 60) return `${totalSeconds} s`
  const totalMinutes = Math.floor(totalSeconds / 60)
  if (totalMinutes < 60) return `${totalMinutes} m ${totalSeconds % 60} s`
  return `${Math.floor(totalMinutes / 60)} h ${totalMinutes % 60} m`
}

const pad = (n: number): string => String(n).padStart(2, '0')

/** Clock for same-day timestamps, `DD.MM HH:MM` otherwise. */
export function formatWhen(ts: number, now: number): string {
  const d = new Date(ts)
  const n = new Date(now)
  const clock = `${pad(d.getHours())}:${pad(d.getMinutes())}`
  const sameDay =
    d.getFullYear() === n.getFullYear() &&
    d.getMonth() === n.getMonth() &&
    d.getDate() === n.getDate()
  return sameDay ? clock : `${pad(d.getDate())}.${pad(d.getMonth() + 1)} ${clock}`
}
