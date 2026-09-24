/**
 * Strips a credential out of a message before it is persisted, logged, or returned to the
 * renderer. Provider error paths surface upstream response bodies verbatim, and a misconfigured
 * reverse proxy in front of a self-hosted tracker/CI instance can echo request headers back on
 * 4xx — so the body is treated as untrusted with respect to the token it was sent with.
 *
 * The falsy guard matters: `replaceAll('')` splices the marker between every character, which
 * would corrupt every message for providers that have no token.
 */
export function redactSecret(message: string, secret?: string): string {
  return secret ? message.replaceAll(secret, '[redacted]') : message
}
