import type { HostStore } from './persistence'
import type { GrpcTransport } from './transport'
import { methods, type RpcTypes, type CommandReceipt } from './schema'
import { encode, decode, toBase64, fromBase64 } from './protobuf'
export const chatMutationMethods = ['ChatService/SendMessage', 'ChatService/EditMessage', 'ChatService/ForkChat', 'ChatService/RunShell', 'ChatService/StopShell'] as const
export type ChatMutationMethod = typeof chatMutationMethods[number]
export function chatRequest(method: keyof RpcTypes, request: unknown): string | undefined {
  if (typeof request !== 'object' || request === null) throw Error('Invalid chat request')
  return chatMutationMethods.includes(method as ChatMutationMethod) ? toBase64(encode(methods[method].input, request)) : undefined
}
export function restoreChatRequest(method: string, bytes: string): RpcTypes[ChatMutationMethod]['input'] {
  if (!chatMutationMethods.includes(method as ChatMutationMethod)) throw Error('Not a replayable chat operation')
  return decode(methods[method as ChatMutationMethod].input, fromBase64(bytes))
}
export async function retryChatMutation(store: HostStore, transport: GrpcTransport, id: string): Promise<CommandReceipt> {
  const state = store.snapshot(), record = state.mutations?.find(value => value.commandId === id)
  if (!record?.chatRequest || state.closingEpoch !== undefined) throw Error('Chat operation unavailable or epoch closing')
  const request = restoreChatRequest(record.method, record.chatRequest), meta = request.command
  if (!meta || meta.command_id !== id || meta.command_epoch !== record.epoch || meta.host_id !== state.connection.hostId || request.chat?.host_id !== meta.host_id) throw Error('Saved command identity mismatch')
  let receipt = await transport.unary('HostService/GetCommand', { host_id: meta.host_id, command_id: id, command_epoch: meta.command_epoch })
  const validate = () => { if (receipt.host_id !== meta.host_id || receipt.command_id !== id || receipt.command_epoch !== meta.command_epoch) throw Error('Recovery receipt identity mismatch') }
  validate()
  if (receipt.state === 8) {
    if (meta.command_epoch !== state.epoch) throw Error('Command epoch retired; intent retained')
    receipt = await transport.unary(record.method as ChatMutationMethod, request)
    validate()
  }
  await store.update(state => { const current = state.mutations?.find(value => value.commandId === id); if (!current) throw Error('Operation journal disappeared'); current.receipt = receipt; return null })
  return receipt
}
