// Keep in sync with src/renderer-shared/rpc/protocol.ts when the wire protocol changes.

/**
 * Constants describing the WebRTC remote-control wire protocol.
 *
 * Three data channels are negotiated by the remote peer (phone, tablet, or
 * another laptop connecting to this Canopy host):
 * - `commands` — reliable ordered, used for RPC request/response and control flow
 * - `state`    — reliable ordered, used for state snapshot + delta events
 * - `stream`   — unreliable ordered, used for hot streams (PTY output, agent text)
 *                where dropping under load is preferable to head-of-line blocking
 */

export const REMOTE_PROTOCOL_VERSION = '1.0.0'

export const CHANNEL_COMMANDS = 'commands'
export const CHANNEL_STATE = 'state'
export const CHANNEL_STREAM = 'stream'

export type ChannelName = typeof CHANNEL_COMMANDS | typeof CHANNEL_STATE | typeof CHANNEL_STREAM
