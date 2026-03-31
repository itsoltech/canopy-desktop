import { SvelteMap } from 'svelte/reactivity'
import { getPref } from './preferences.svelte'

const MAX_ENTRIES = 10
const EXPIRE_MS = 2000
/** Keys in the last N ms count toward "burst" speed */
const BURST_WINDOW_MS = 1500

export type TypingIntensity = 'idle' | 'normal' | 'fast' | 'blazing'

interface KeyDisplay {
  id: number
  label: string
  timestamp: number
}

const sessions = new SvelteMap<string, KeyDisplay[]>()
const timers: Record<number, ReturnType<typeof setTimeout>> = {}
const intensityMap: Record<string, TypingIntensity> = $state({})

let nextId = 0

const isMac = typeof navigator !== 'undefined' && navigator.userAgent.includes('Mac')

const MODIFIER_KEYS = new Set(['Control', 'Shift', 'Alt', 'Meta'])

const KEY_LABELS: Record<string, string> = {
  ArrowUp: '\u2191',
  ArrowDown: '\u2193',
  ArrowLeft: '\u2190',
  ArrowRight: '\u2192',
  Backspace: '\u232B',
  Delete: 'Del',
  Enter: '\u21B5',
  Escape: 'Esc',
  Tab: '\u21E5',
  ' ': 'Space',
  CapsLock: 'Caps',
  Home: 'Home',
  End: 'End',
  PageUp: 'PgUp',
  PageDown: 'PgDn',
  Insert: 'Ins',
}

function formatKeyLabel(event: KeyboardEvent): string | null {
  if (MODIFIER_KEYS.has(event.key)) return null

  const parts: string[] = []

  if (event.ctrlKey) parts.push('Ctrl')
  if (event.altKey) parts.push('Alt')
  if (isMac && event.metaKey) parts.push('Cmd')
  if (!isMac && event.metaKey) parts.push('Win')

  const mapped = KEY_LABELS[event.key]
  const isSpecialKey = mapped !== undefined || event.key.length > 1

  // Only show Shift for non-character keys (Shift+Enter, Shift+Tab, etc.)
  if (event.shiftKey && isSpecialKey) parts.push('Shift')

  if (mapped) {
    parts.push(mapped)
  } else if (event.key.length === 1) {
    parts.push(event.key)
  } else {
    parts.push(event.key)
  }

  return parts.join('+')
}

function computeIntensity(queue: KeyDisplay[]): TypingIntensity {
  if (queue.length < 2) return 'normal'
  const now = Date.now()
  const recentCount = queue.filter((k) => now - k.timestamp < BURST_WINDOW_MS).length
  if (recentCount >= 8) return 'blazing'
  if (recentCount >= 5) return 'fast'
  return 'normal'
}

function removeEntry(sessionId: string, entryId: number): void {
  const queue = sessions.get(sessionId)
  if (!queue) return
  const idx = queue.findIndex((e) => e.id === entryId)
  if (idx !== -1) {
    queue.splice(idx, 1)
    const newQueue = [...queue]
    sessions.set(sessionId, newQueue)
    intensityMap[sessionId] = computeIntensity(newQueue)
  }
  delete timers[entryId]
}

export function recordKeyEvent(sessionId: string, event: KeyboardEvent): void {
  if (getPref('keystrokeVisualizer.enabled') !== 'true') return
  if (event.repeat) return

  const label = formatKeyLabel(event)
  if (!label) return

  const id = nextId++
  const entry: KeyDisplay = { id, label, timestamp: Date.now() }

  let queue = sessions.get(sessionId)
  if (!queue) {
    queue = []
    sessions.set(sessionId, queue)
  }

  // Evict oldest if at capacity
  while (queue.length >= MAX_ENTRIES) {
    const removed = queue.shift()!
    if (timers[removed.id]) {
      clearTimeout(timers[removed.id])
      delete timers[removed.id]
    }
  }

  queue.push(entry)
  const newQueue = [...queue]
  sessions.set(sessionId, newQueue)
  intensityMap[sessionId] = computeIntensity(newQueue)

  timers[id] = setTimeout(() => removeEntry(sessionId, id), EXPIRE_MS)
}

export function getKeystrokes(sessionId: string): KeyDisplay[] {
  return sessions.get(sessionId) ?? []
}

export function getIntensity(sessionId: string): TypingIntensity {
  return intensityMap[sessionId] ?? 'idle'
}

export function cleanupKeystrokeSession(sessionId: string): void {
  const queue = sessions.get(sessionId)
  if (queue) {
    for (const entry of queue) {
      if (timers[entry.id]) {
        clearTimeout(timers[entry.id])
        delete timers[entry.id]
      }
    }
  }
  sessions.delete(sessionId)
  delete intensityMap[sessionId]
}
