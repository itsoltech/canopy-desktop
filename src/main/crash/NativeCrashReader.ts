import { readdirSync, readFileSync, statSync } from 'fs'
import os from 'os'
import { join } from 'path'

export interface NativeCrashInfo {
  timestamp: string
  processName?: string
  exceptionType?: string
  exceptionCodes?: string
  terminationReason?: string
  triggeredThread?: string
  stack: string
  incidentId?: string
}

interface IpsFrame {
  imageIndex?: number
  imageOffset?: number
  symbol?: string
  symbolLocation?: number
}

interface IpsThread {
  name?: string
  queue?: string
  triggered?: boolean
  frames?: IpsFrame[]
}

interface IpsImage {
  name?: string
}

interface IpsHeader {
  app_name?: string
  timestamp?: string
  incident_id?: string
}

interface IpsBody {
  captureTime?: string
  procName?: string
  exception?: {
    type?: string
    signal?: string
    codes?: string
  }
  termination?: {
    indicator?: string
    byProc?: string
  }
  threads?: IpsThread[]
  usedImages?: IpsImage[]
}

const MAX_FRAMES = 40
const MAX_STACK_CHARS = 4000
const CLOCK_SKEW_MS = 60_000
const REDACTED = '[redacted]'
const URL_PATTERN = /\bhttps?:\/\/[^\s<>"'`]+/gi
const USER_PATH_PATTERN = /(?:[A-Z]:\\Users\\[^\\\s]+\\|\/(?:Users|home)\/[^/\s]+\/)/gi
const TOKEN_LIKE_PATTERN =
  /\b(?:token|secret|password|passwd|apikey|api_key|authorization|bearer)[A-Za-z0-9_\-:=./+]{3,}/gi
const ENV_VALUE_PATTERN = /\b[A-Z][A-Z0-9_]{2,}=([^\s]+)/g

export function findRecentNativeCrash(
  processName: string,
  sinceEpochMs: number,
  nowEpochMs: number = Date.now(),
): NativeCrashInfo | null {
  if (process.platform !== 'darwin') return null

  try {
    return findCandidate(processName, sinceEpochMs, nowEpochMs)
  } catch {
    return null
  }
}

function findCandidate(
  processName: string,
  sinceEpochMs: number,
  nowEpochMs: number,
): NativeCrashInfo | null {
  const home = os.homedir()
  const dirs = [
    join(home, 'Library', 'Logs', 'DiagnosticReports'),
    join(home, 'Library', 'Logs', 'DiagnosticReports', 'Retired'),
  ]

  const lowerBound = sinceEpochMs - CLOCK_SKEW_MS
  const upperBound = nowEpochMs + CLOCK_SKEW_MS
  const prefix = `${processName}-`

  let best: { info: NativeCrashInfo; crashMs: number; mtimeMs: number } | null = null

  for (const dir of dirs) {
    let entries: string[]
    try {
      entries = readdirSync(dir)
    } catch {
      continue
    }

    for (const entry of entries) {
      if (!entry.endsWith('.ips')) continue
      if (!entry.startsWith(prefix)) continue

      const fullPath = join(dir, entry)
      let mtimeMs: number
      try {
        mtimeMs = statSync(fullPath).mtimeMs
      } catch {
        continue
      }

      if (mtimeMs < lowerBound) continue

      const info = parseIpsFile(fullPath)
      if (!info) continue
      if (info.processName !== processName) continue

      const crashMs = Date.parse(info.timestamp)
      if (Number.isNaN(crashMs)) continue
      if (crashMs < lowerBound || crashMs > upperBound) continue

      if (!best || crashMs > best.crashMs || (crashMs === best.crashMs && mtimeMs > best.mtimeMs)) {
        best = { info, crashMs, mtimeMs }
      }
    }
  }

  return best?.info ?? null
}

function parseIpsFile(filePath: string): NativeCrashInfo | null {
  let raw: string
  try {
    raw = readFileSync(filePath, 'utf-8')
  } catch {
    return null
  }

  const newlineIdx = raw.indexOf('\n')
  if (newlineIdx < 0) return null

  let header: IpsHeader
  let body: IpsBody
  try {
    header = JSON.parse(raw.slice(0, newlineIdx)) as IpsHeader
    body = JSON.parse(raw.slice(newlineIdx + 1).trim()) as IpsBody
  } catch {
    return null
  }

  const triggered = (body.threads ?? []).find((t) => t.triggered === true)
  const stack = formatFrames(triggered?.frames ?? [], body.usedImages ?? [])

  const exceptionType = formatException(body.exception)
  const terminationReason = formatTermination(body.termination)

  return {
    timestamp: toIsoTimestamp(body.captureTime ?? header.timestamp),
    processName: body.procName ?? header.app_name,
    exceptionType,
    exceptionCodes: sanitizeDiagnosticText(body.exception?.codes),
    terminationReason,
    triggeredThread: sanitizeDiagnosticText(triggered?.name ?? triggered?.queue),
    stack,
    incidentId: sanitizeDiagnosticText(header.incident_id),
  }
}

function toIsoTimestamp(raw: string | undefined): string {
  if (raw) {
    const ms = Date.parse(raw)
    if (!Number.isNaN(ms)) return new Date(ms).toISOString()
  }
  return new Date().toISOString()
}

function formatException(exception: IpsBody['exception']): string | undefined {
  if (!exception) return undefined
  const { type, signal } = exception
  if (type && signal) return sanitizeDiagnosticText(`${type} (${signal})`)
  return sanitizeDiagnosticText(type ?? signal)
}

function formatTermination(termination: IpsBody['termination']): string | undefined {
  if (!termination) return undefined
  const { indicator, byProc } = termination
  if (indicator && byProc) return sanitizeDiagnosticText(`${indicator} (by ${byProc})`)
  return sanitizeDiagnosticText(indicator ?? byProc)
}

function formatFrames(frames: IpsFrame[], images: IpsImage[]): string {
  const lines: string[] = []

  for (let i = 0; i < frames.length && i < MAX_FRAMES; i++) {
    const frame = frames[i]
    const imageName = sanitizeFramePart(
      frame.imageIndex !== undefined ? (images[frame.imageIndex]?.name ?? '???') : '???',
    )
    const symbol = sanitizeFramePart(frame.symbol ?? '???')
    const offset = frame.symbolLocation ?? 0
    const idx = String(i).padStart(3, ' ')
    const img = imageName.padEnd(32, ' ')
    lines.push(`${idx}  ${img}  ${symbol} + ${offset}`)
  }

  const joined = lines.join('\n')
  if (joined.length <= MAX_STACK_CHARS) return joined
  return `${joined.slice(0, MAX_STACK_CHARS - 20)}\n... (truncated)`
}

function sanitizeFramePart(value: string): string {
  const sanitized = sanitizeDiagnosticText(value) ?? '???'
  if (sanitized.length <= 160) return sanitized
  return `${sanitized.slice(0, 157)}...`
}

function sanitizeDiagnosticText(value: string | undefined): string | undefined {
  if (!value) return value
  return value
    .replace(USER_PATH_PATTERN, '~/')
    .replace(URL_PATTERN, REDACTED)
    .replace(TOKEN_LIKE_PATTERN, REDACTED)
    .replace(ENV_VALUE_PATTERN, (match) => {
      const idx = match.indexOf('=')
      return idx >= 0 ? `${match.slice(0, idx + 1)}${REDACTED}` : REDACTED
    })
}
