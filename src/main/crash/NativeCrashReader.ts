import { readdirSync, readFileSync, statSync } from 'fs'
import os from 'os'
import { join } from 'path'

export interface NativeCrashInfo {
  timestamp: string
  exceptionType?: string
  exceptionCodes?: string
  terminationReason?: string
  triggeredThread?: string
  stack: string
  incidentId?: string
  sourceFile: string
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

export function findRecentNativeCrash(
  processName: string,
  sinceEpochMs: number,
  nowEpochMs: number = Date.now(),
): NativeCrashInfo | null {
  if (process.platform !== 'darwin') return null

  try {
    const candidate = findCandidateFile(processName, sinceEpochMs, nowEpochMs)
    if (!candidate) return null
    return parseIpsFile(candidate)
  } catch {
    return null
  }
}

function findCandidateFile(
  processName: string,
  sinceEpochMs: number,
  nowEpochMs: number,
): string | null {
  const home = os.homedir()
  const dirs = [
    join(home, 'Library', 'Logs', 'DiagnosticReports'),
    join(home, 'Library', 'Logs', 'DiagnosticReports', 'Retired'),
  ]

  const lowerBound = sinceEpochMs - CLOCK_SKEW_MS
  const upperBound = nowEpochMs + CLOCK_SKEW_MS
  const prefix = `${processName}-`

  let best: { path: string; mtimeMs: number } | null = null

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

      if (mtimeMs < lowerBound || mtimeMs > upperBound) continue

      if (!best || mtimeMs > best.mtimeMs) {
        best = { path: fullPath, mtimeMs }
      }
    }
  }

  return best ? best.path : null
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
    body = JSON.parse(raw.slice(newlineIdx + 1)) as IpsBody
  } catch {
    return null
  }

  const triggered = (body.threads ?? []).find((t) => t.triggered === true)
  const stack = formatFrames(triggered?.frames ?? [], body.usedImages ?? [])

  const exceptionType = formatException(body.exception)
  const terminationReason = formatTermination(body.termination)

  return {
    timestamp: toIsoTimestamp(header.timestamp),
    exceptionType,
    exceptionCodes: body.exception?.codes,
    terminationReason,
    triggeredThread: triggered?.name ?? triggered?.queue,
    stack,
    incidentId: header.incident_id,
    sourceFile: filePath,
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
  if (type && signal) return `${type} (${signal})`
  return type ?? signal
}

function formatTermination(termination: IpsBody['termination']): string | undefined {
  if (!termination) return undefined
  const { indicator, byProc } = termination
  if (indicator && byProc) return `${indicator} (by ${byProc})`
  return indicator ?? byProc
}

function formatFrames(frames: IpsFrame[], images: IpsImage[]): string {
  const lines: string[] = []

  for (let i = 0; i < frames.length && i < MAX_FRAMES; i++) {
    const frame = frames[i]
    const imageName =
      frame.imageIndex !== undefined ? (images[frame.imageIndex]?.name ?? '???') : '???'
    const symbol = frame.symbol ?? '???'
    const offset = frame.symbolLocation ?? 0
    const idx = String(i).padStart(3, ' ')
    const img = imageName.padEnd(32, ' ')
    lines.push(`${idx}  ${img}  ${symbol} + ${offset}`)
  }

  const joined = lines.join('\n')
  if (joined.length <= MAX_STACK_CHARS) return joined
  return `${joined.slice(0, MAX_STACK_CHARS - 20)}\n… (truncated)`
}
