import { describe, expect, it } from 'vitest'
import {
  DESTRUCTIVE_METHODS,
  SESSION_GRANTABLE_METHODS,
  type RpcMethodName,
} from '../../../../renderer-shared/rpc/methodList'

/**
 * RPC methods that expose or inject live terminal content. Terminal streams
 * carry whatever the host user is doing — tokens echoed by CLIs, `env` dumps,
 * agent conversations, source — so *reading* one is at least as sensitive as
 * writing to it. Every method here must be gated by `checkAction`, otherwise a
 * paired peer can enumerate sessions via `state.getSnapshot` and stream them
 * with no host prompt under the default `destructive` guard profile.
 */
const TERMINAL_CONTENT_METHODS: RpcMethodName[] = ['pty.write', 'agent.sendInput', 'pty.subscribe']

describe('remote action-guard coverage', () => {
  it.each(TERMINAL_CONTENT_METHODS)('gates "%s" behind host consent', (method) => {
    const gated = SESSION_GRANTABLE_METHODS.has(method) || DESTRUCTIVE_METHODS.has(method)
    expect(gated).toBe(true)
  })

  it('keeps continuous-input methods out of DESTRUCTIVE_METHODS', () => {
    // Anything on a keystroke/stream path would fire a confirm modal per call.
    for (const method of SESSION_GRANTABLE_METHODS) {
      expect(DESTRUCTIVE_METHODS.has(method)).toBe(false)
    }
  })
})
