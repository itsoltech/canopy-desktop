import { match } from 'ts-pattern'

/**
 * Typed error union surfaced by the SDK-agent service. Every consumer formats
 * these through `sdkAgentErrorMessage()` so the renderer never sees raw
 * exception text.
 */
export type SdkAgentError =
  | { _tag: 'auth_missing' }
  | { _tag: 'auth_invalid'; reason?: string }
  | { _tag: 'rate_limited'; retryAfterMs?: number }
  | { _tag: 'network'; cause: string }
  | { _tag: 'aborted' }
  | { _tag: 'sdk_internal'; message: string }
  | { _tag: 'profile_not_found'; profileId?: string }
  | { _tag: 'unknown'; message: string }

export function sdkAgentErrorMessage(error: SdkAgentError): string {
  return match(error)
    .with({ _tag: 'auth_missing' }, () => 'No API key configured for this profile.')
    .with({ _tag: 'auth_invalid' }, ({ reason }) =>
      reason ? `Authentication rejected: ${reason}` : 'Authentication rejected by the API.',
    )
    .with({ _tag: 'rate_limited' }, ({ retryAfterMs }) =>
      retryAfterMs
        ? `Rate limited — retry in ${Math.ceil(retryAfterMs / 1000)}s.`
        : 'Rate limited by the API. Retrying automatically.',
    )
    .with({ _tag: 'network' }, ({ cause }) => `Network error: ${cause}`)
    .with({ _tag: 'aborted' }, () => 'Request aborted.')
    .with({ _tag: 'sdk_internal' }, ({ message }) => `SDK error: ${message}`)
    .with({ _tag: 'profile_not_found' }, ({ profileId }) =>
      profileId ? `Agent profile not found: ${profileId}` : 'Agent profile not found.',
    )
    .with({ _tag: 'unknown' }, ({ message }) => message)
    .exhaustive()
}

/**
 * Best-effort mapping from an unknown thrown value into our typed union.
 * Live providers should prefer narrow handling; this is the fallback used by
 * `fromExternalCall()` wrappers.
 */
export function toSdkAgentError(e: unknown): SdkAgentError {
  if (e instanceof Error) {
    if (e.name === 'AbortError') return { _tag: 'aborted' }
    return { _tag: 'unknown', message: e.message }
  }
  return { _tag: 'unknown', message: String(e) }
}
