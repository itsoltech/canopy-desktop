import { SvelteMap } from 'svelte/reactivity'
import { getPref } from './preferences.svelte'

const WINDOW_MS = 10_000
const CLEANUP_INTERVAL_MS = 5_000
const CHARS_PER_WORD = 5
/** Minimum printable chars before we consider the session "active" */
const MIN_CHARS_TO_ACTIVATE = 5
/** Minimum keystrokes within the window to show WPM (filters startup noise) */
const MIN_WINDOW_CHARS = 3

interface SessionData {
  timestamps: number[]
  lastActivity: number
  totalChars: number
  sessionStart: number
  peakWpm: number
}

const sessions = new SvelteMap<string, SessionData>()

// All reactive values exposed to components — $state Records for fine-grained reactivity
const wpmValues: Record<string, number> = $state({})
const peakValues: Record<string, number> = $state({})
const totalCharsValues: Record<string, number> = $state({})
const activeFlags: Record<string, boolean> = $state({})

/** Only count real keystrokes — skip escape sequences (mouse events, arrows, function keys) */
function countPrintable(data: string): number {
  // Escape sequences (mouse reports, arrow keys, function keys) start with \x1b
  if (data.charCodeAt(0) === 0x1b) return 0
  // Single control chars (Enter, Tab, Backspace, Ctrl+C, etc.)
  if (data.length === 1 && data.charCodeAt(0) < 32) return 0

  let count = 0
  for (let i = 0; i < data.length; i++) {
    const code = data.charCodeAt(i)
    if (code >= 32 && code < 127) count++
    else if (code > 127) count++
  }
  return count
}

export function recordKeystroke(sessionId: string, data: string): void {
  if (getPref('wpm.enabled') !== 'true') return

  const printable = countPrintable(data)
  if (printable === 0) return

  const now = Date.now()
  let session = sessions.get(sessionId)
  if (!session) {
    session = { timestamps: [], lastActivity: now, totalChars: 0, sessionStart: now, peakWpm: 0 }
    sessions.set(sessionId, session)
  }

  for (let i = 0; i < printable; i++) {
    session.timestamps.push(now)
  }
  session.lastActivity = now
  session.totalChars += printable

  // Update reactive flags
  totalCharsValues[sessionId] = session.totalChars
  if (session.totalChars >= MIN_CHARS_TO_ACTIVATE) {
    activeFlags[sessionId] = true
  }

  // Trim old timestamps outside window
  const cutoff = now - WINDOW_MS
  const idx = session.timestamps.findIndex((t) => t >= cutoff)
  if (idx > 0) session.timestamps.splice(0, idx)

  // Need enough data points for a meaningful WPM reading
  if (session.timestamps.length < MIN_WINDOW_CHARS) {
    wpmValues[sessionId] = 0
    return
  }

  const chars = session.timestamps.length
  const elapsed = Math.min(now - session.timestamps[0], WINDOW_MS)

  // Require at least 2 seconds of typing data to avoid inflated initial readings
  if (elapsed < 2000) {
    wpmValues[sessionId] = 0
    return
  }

  const minutes = elapsed / 60_000
  const wpm = minutes > 0 ? Math.round(chars / CHARS_PER_WORD / minutes) : 0
  wpmValues[sessionId] = wpm

  if (wpm > session.peakWpm) {
    session.peakWpm = wpm
    peakValues[sessionId] = wpm
  }
}

export function getWpm(sessionId: string): number {
  return wpmValues[sessionId] ?? 0
}

export function getLastActivity(sessionId: string): number {
  return sessions.get(sessionId)?.lastActivity ?? 0
}

export function isSessionActive(sessionId: string): boolean {
  return activeFlags[sessionId] ?? false
}

export function getSessionStats(sessionId: string): {
  totalChars: number
  peakWpm: number
} {
  return {
    totalChars: totalCharsValues[sessionId] ?? 0,
    peakWpm: peakValues[sessionId] ?? 0,
  }
}

export function cleanupSession(sessionId: string): void {
  sessions.delete(sessionId)
  delete wpmValues[sessionId]
  delete peakValues[sessionId]
  delete totalCharsValues[sessionId]
  delete activeFlags[sessionId]
}

export function resetAllSessions(): void {
  for (const id of sessions.keys()) {
    delete wpmValues[id]
    delete peakValues[id]
    delete totalCharsValues[id]
    delete activeFlags[id]
  }
  sessions.clear()
}

let cleanupIntervalId: ReturnType<typeof setInterval> | undefined

export function startCleanupTimer(): void {
  stopCleanupTimer()
  cleanupIntervalId = setInterval(() => {
    const now = Date.now()
    for (const [id, session] of sessions) {
      if (now - session.lastActivity > 30_000) {
        sessions.delete(id)
        delete wpmValues[id]
        delete peakValues[id]
        delete totalCharsValues[id]
        delete activeFlags[id]
      }
    }
  }, CLEANUP_INTERVAL_MS)
}

export function stopCleanupTimer(): void {
  clearInterval(cleanupIntervalId)
}

startCleanupTimer()

if (import.meta.hot) {
  import.meta.hot.dispose(() => stopCleanupTimer())
}
