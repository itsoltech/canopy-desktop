import type { HostStore } from './persistence'
import type { CommandMeta, CommandReceipt } from './schema'
export function uniqueId(): string {
  const bytes = new Uint8Array(16); crypto.getRandomValues(bytes)
  bytes[6] = (bytes[6] & 15) | 64; bytes[8] = (bytes[8] & 63) | 128
  const h = Array.from(bytes, b => b.toString(16).padStart(2, '0')).join('')
  return `${h.slice(0,8)}-${h.slice(8,12)}-${h.slice(12,16)}-${h.slice(16,20)}-${h.slice(20)}`
}
export function command(store: HostStore, revision?: string): CommandMeta {
  const state = store.snapshot()
  if (state.epoch === undefined || state.closingEpoch !== undefined) throw Error('Command epoch is unavailable')
  return { command_id: uniqueId(), command_epoch: state.epoch, expected_revision: revision }
}
export function checkReceipt(receipt: CommandReceipt): CommandReceipt {
  if (receipt.error) throw Error(receipt.error.message)
  if ([4,5,6].includes(receipt.state)) throw Error(`Operation state ${receipt.state}; reconcile before retrying`)
  return receipt
}
