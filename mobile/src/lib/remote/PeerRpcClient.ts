// Keep in sync with src/renderer/src/remote/lib/peer/PeerRpcClient.ts when the wire protocol changes.

import type { DataChannelRpc } from './DataChannelRpc'
import type { CallArgs, RpcMethodName, RpcMethods } from './protocol/rpc-methods'

/**
 * Thin typed façade over {@link DataChannelRpc} for the remote peer.
 *
 * The underlying `DataChannelRpc` is intentionally untyped at the wire level
 * so the same class can serve both directions; this wrapper re-introduces
 * the method-name-indexed `RpcMethods` map so callers get IntelliSense and
 * compile-time argument/return checks.
 */
export class PeerRpcClient {
  constructor(private rpc: DataChannelRpc) {}

  call<M extends RpcMethodName>(method: M, ...args: CallArgs<M>): Promise<RpcMethods[M]['result']> {
    const params = args.length === 0 ? undefined : args[0]
    return this.rpc.call<RpcMethods[M]['result']>(method, params)
  }

  subscribe<T = unknown>(topic: string, handler: (data: T) => void): () => void {
    return this.rpc.subscribe(topic, handler as (data: unknown) => void)
  }
}
