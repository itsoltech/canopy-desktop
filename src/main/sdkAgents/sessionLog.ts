import fs from 'fs'
import path from 'path'
import { app } from 'electron'
import type { ConversationId, SdkAgentEvent } from './types'

const SEVEN_DAYS_MS = 7 * 24 * 60 * 60 * 1000

function logsDir(): string {
  return path.join(app.getPath('userData'), 'logs', 'sdk')
}

/**
 * Append-only JSONL logger for a single session. One instance per
 * ConversationId. The file lives at `<userData>/logs/sdk/<id>.log` and is
 * closed when the session ends. Each line is a compact JSON object with
 * `ts`, `_tag`, and the event payload minus the large `content`/`input`
 * fields (redacted) so disk footprint stays small.
 */
export class SessionLog {
  private stream: fs.WriteStream | null = null
  private readonly filePath: string

  constructor(conversationId: ConversationId) {
    const dir = logsDir()
    fs.mkdirSync(dir, { recursive: true })
    this.filePath = path.join(dir, `${conversationId}.log`)
  }

  open(): void {
    if (this.stream) return
    this.stream = fs.createWriteStream(this.filePath, { flags: 'a' })
  }

  write(event: SdkAgentEvent): void {
    if (!this.stream) this.open()
    const redacted = redact(event)
    const line = JSON.stringify({ ts: new Date().toISOString(), ...redacted }) + '\n'
    this.stream?.write(line)
  }

  close(): void {
    this.stream?.end()
    this.stream = null
  }
}

/**
 * Redact high-volume / potentially sensitive payload fields so the on-disk
 * log is useful for debugging without hoarding user data. Raw prompts and
 * tool inputs are reduced to length / hash metadata.
 */
function redact(event: SdkAgentEvent): Record<string, unknown> {
  const shallow: Record<string, unknown> = { ...(event as Record<string, unknown>) }
  if ('content' in shallow && Array.isArray(shallow.content)) {
    shallow.content = `[${shallow.content.length} blocks]`
  }
  if ('input' in shallow && shallow.input && typeof shallow.input === 'object') {
    shallow.input = `[${Object.keys(shallow.input).length} keys]`
  }
  if ('result' in shallow && typeof shallow.result === 'string') {
    const s = shallow.result
    shallow.result = s.length > 200 ? `${s.slice(0, 200)}…(${s.length} chars)` : s
  }
  if ('plan' in shallow && typeof shallow.plan === 'string') {
    shallow.plan = `[plan: ${shallow.plan.length} chars]`
  }
  return shallow
}

/**
 * Delete session log files older than 7 days. Intended to run once at
 * app startup — no scheduler needed. Missing directory is a no-op.
 */
export function sweepOldSessionLogs(now = Date.now()): { deleted: number } {
  const dir = logsDir()
  let deleted = 0
  let entries: fs.Dirent[]
  try {
    entries = fs.readdirSync(dir, { withFileTypes: true })
  } catch {
    return { deleted }
  }
  for (const entry of entries) {
    if (!entry.isFile() || !entry.name.endsWith('.log')) continue
    const full = path.join(dir, entry.name)
    try {
      const stat = fs.statSync(full)
      if (now - stat.mtimeMs > SEVEN_DAYS_MS) {
        fs.unlinkSync(full)
        deleted++
      }
    } catch {
      // Best-effort — unreadable entries stay.
    }
  }
  return { deleted }
}
