import { match } from 'ts-pattern'
import type { SdkAgentEvent } from './types'

/** Exponential backoff schedule, in ms. */
const DEFAULT_BACKOFF = [1_000, 4_000, 16_000] as const

export interface RetryOptions {
  maxAttempts?: number
  backoffMs?: readonly number[]
}

/**
 * Wrap an SDK-event async iterable producer with 429/5xx retry + exponential
 * backoff. A `retry-after` header (seconds or HTTP-date) takes precedence
 * over the schedule when present. Errors that don't look retryable bubble
 * out immediately.
 *
 * The producer is called on every attempt so it can re-open the underlying
 * SDK query. Aborts propagate unchanged.
 */
export async function* withRetry(
  produce: (attempt: number) => AsyncIterable<SdkAgentEvent>,
  options: RetryOptions = {},
): AsyncIterable<SdkAgentEvent> {
  const backoff = options.backoffMs ?? DEFAULT_BACKOFF
  const maxAttempts = options.maxAttempts ?? backoff.length + 1

  let attempt = 0
  while (true) {
    const iter = produce(attempt)
    try {
      for await (const ev of iter) yield ev
      return
    } catch (e) {
      if (!isRetryable(e) || attempt >= maxAttempts - 1) throw e
      const delay = pickDelay(e, backoff, attempt)
      await sleep(delay)
      attempt++
    }
  }
}

function isRetryable(e: unknown): boolean {
  const status = readStatus(e)
  if (status === 429) return true
  if (status !== null && status >= 500 && status < 600) return true
  // Transient network errors expose codes like ECONNRESET / ETIMEDOUT.
  const code = readCode(e)
  if (code && ['ECONNRESET', 'ETIMEDOUT', 'ENETDOWN', 'ENETUNREACH'].includes(code)) return true
  return false
}

function pickDelay(e: unknown, backoff: readonly number[], attempt: number): number {
  const retryAfter = readRetryAfter(e)
  const base = backoff[Math.min(attempt, backoff.length - 1)]
  if (retryAfter !== null) return Math.max(retryAfter, base)
  return base
}

function readStatus(e: unknown): number | null {
  if (!e || typeof e !== 'object') return null
  const withStatus = e as {
    status?: unknown
    statusCode?: unknown
    response?: { status?: unknown }
  }
  const direct = withStatus.status ?? withStatus.statusCode
  if (typeof direct === 'number') return direct
  if (withStatus.response && typeof withStatus.response.status === 'number') {
    return withStatus.response.status
  }
  return null
}

function readCode(e: unknown): string | null {
  if (!e || typeof e !== 'object') return null
  const v = (e as { code?: unknown }).code
  return typeof v === 'string' ? v : null
}

/**
 * Read a retry-after header value (seconds integer or HTTP-date). Returns ms.
 */
function readRetryAfter(e: unknown): number | null {
  if (!e || typeof e !== 'object') return null
  const candidates = [
    (e as { retryAfter?: unknown }).retryAfter,
    (e as { headers?: { 'retry-after'?: unknown } }).headers?.['retry-after'],
    (e as { response?: { headers?: { 'retry-after'?: unknown } } }).response?.headers?.[
      'retry-after'
    ],
  ]
  for (const c of candidates) {
    const parsed = parseRetryAfter(c)
    if (parsed !== null) return parsed
  }
  return null
}

function parseRetryAfter(raw: unknown): number | null {
  return match(raw)
    .with(undefined, null, () => null)
    .when(
      (v): v is number => typeof v === 'number',
      (v) => v * 1000,
    )
    .when(
      (v): v is string => typeof v === 'string',
      (v) => {
        const asInt = Number.parseInt(v, 10)
        if (!Number.isNaN(asInt) && String(asInt) === v.trim()) return asInt * 1000
        const asDate = Date.parse(v)
        if (!Number.isNaN(asDate)) return Math.max(0, asDate - Date.now())
        return null
      },
    )
    .otherwise(() => null)
}

function sleep(ms: number): Promise<void> {
  return new Promise((resolve) => setTimeout(resolve, ms))
}
