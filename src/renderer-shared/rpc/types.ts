/**
 * Wire protocol for RPC over RTCDataChannel between Canopy host and remote peer.
 *
 * Three message kinds:
 * - `request`  — caller initiates an RPC; expects exactly one matching `response`.
 * - `response` — replies to a `request` by id; either `ok: true` with `result`
 *                or `ok: false` with a tagged `error`.
 * - `event`    — server-pushed update on a topic; no response expected.
 *
 * The protocol is intentionally JSON-friendly so it can be carried over any
 * text-mode transport (data channel, WebSocket, HTTP long-poll) without
 * binary encoding concerns.
 */

export type RpcRequest = {
  kind: 'request'
  id: string
  method: string
  params: unknown
}

export type RpcSuccessResponse = {
  kind: 'response'
  id: string
  ok: true
  result: unknown
}

export type RpcErrorResponse = {
  kind: 'response'
  id: string
  ok: false
  error: { _tag: string; message: string }
}

export type RpcResponse = RpcSuccessResponse | RpcErrorResponse

export type RpcEvent = {
  kind: 'event'
  topic: string
  data: unknown
}

export type RpcMessage = RpcRequest | RpcResponse | RpcEvent
