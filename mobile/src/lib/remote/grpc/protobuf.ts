import { messages } from './schema'

type Field = { name: string; number: number; type: number; target: string; repeated: boolean; optional: boolean; oneof?: number; map?: boolean }
const schemas: Record<string, readonly Field[]> = messages
const utf8 = new TextEncoder()
const decoder = new TextDecoder('utf-8', { fatal: true })
const MAX_BYTES = 1024 * 1024

// All field metadata is generated from protoc's descriptor set. No RPC-specific JSON routing.
export function encode(type: string, value: object, depth = 0, maxBytes = MAX_BYTES): Uint8Array {
  if (depth > 64) throw Error('protobuf nesting limit')
  const fields = schemas[type]
  if (!fields) throw Error('unknown protobuf type')
  const out: number[] = []
  const groups = new Set<number>()
  const varint = (n: bigint) => { n = BigInt.asUintN(64, n); while (n > 127n) { out.push(Number(n & 127n) | 128); n >>= 7n }; out.push(Number(n)) }
  for (const f of fields) {
    const v = (value as Record<string, unknown>)[f.name]
    if (v === undefined || v === null) continue
    if (f.oneof !== undefined) { if (groups.has(f.oneof)) throw Error('multiple protobuf oneof values'); groups.add(f.oneof) }
    for (const item of f.map ? Object.entries(v as Record<string, unknown>).map(([key, value]) => ({ key, value })) : f.repeated ? v as unknown[] : [v]) {
      if ([9, 11, 12].includes(f.type)) {
        const bytes = f.type === 9 ? utf8.encode(item as string) : f.type === 11 ? encode(f.target, item as object, depth + 1, maxBytes) : item as Uint8Array
        if (!(bytes instanceof Uint8Array)) throw Error('protobuf bytes required')
        varint(BigInt(f.number * 8 + 2)); varint(BigInt(bytes.length))
        for (const b of bytes) out.push(b)
      } else if (f.type === 1) {
        varint(BigInt(f.number * 8 + 1)); const bytes = new Uint8Array(8)
        new DataView(bytes.buffer).setFloat64(0, item as number, true)
        for (const b of bytes) out.push(b)
      } else if ([3, 4, 5, 8, 13, 14].includes(f.type)) {
        varint(BigInt(f.number * 8)); varint(f.type === 8 ? (item ? 1n : 0n) : BigInt(item as string | number))
      } else throw Error('unsupported protobuf field type')
      if (out.length > maxBytes) throw Error('resource_exhausted: protobuf message')
    }
  }
  return Uint8Array.from(out)
}
export function decode<T>(type: string, bytes: Uint8Array, depth = 0, maxBytes = MAX_BYTES): T {
  if (depth > 64 || bytes.length > maxBytes) throw Error('protobuf limit')
  const fields = schemas[type]
  if (!fields) throw Error('unknown protobuf type')
  const result: Record<string, unknown> = {}
  for (const f of fields) {
    if (f.map) result[f.name] = {}
    else if (f.repeated) result[f.name] = []
    else if (f.optional || f.oneof !== undefined || f.type === 11) continue
    else result[f.name] = f.type === 9 ? '' : f.type === 12 ? new Uint8Array() : f.type === 8 ? false : [3,4].includes(f.type) ? '0' : 0
  }
  let p = 0
  function varint(): bigint {
    let v = 0n
    for (let shift = 0n; shift < 70n; shift += 7n) {
      if (p >= bytes.length) throw Error('truncated protobuf')
      const b = bytes[p++]; if (shift === 63n && b > 1) throw Error('protobuf integer overflow')
      v |= BigInt(b & 127) << shift
      if (!(b & 128)) return v
    }
    throw Error('protobuf integer overflow')
  }
  function take(n: number): Uint8Array {
    if (!Number.isSafeInteger(n) || n < 0 || p + n > bytes.length) throw Error('truncated protobuf')
    const b = bytes.subarray(p, p + n); p += n; return b
  }
  function scalar(f: Field, wire: number): unknown {
    if (wire === 2) {
      const b = take(Number(varint()))
      if (f.type === 9) return decoder.decode(b)
      if (f.type === 12) return b.slice()
      if (f.type === 11) return decode(f.target, b, depth + 1, maxBytes)
    } else if (wire === 1 && f.type === 1) {
      const b = take(8); return new DataView(b.buffer, b.byteOffset, 8).getFloat64(0, true)
    } else if (wire === 0 && [3,4,5,8,13,14].includes(f.type)) {
      const n = varint()
      if (f.type === 3) return BigInt.asIntN(64, n).toString()
      if (f.type === 4) return n.toString()
      if (f.type === 8) return n !== 0n
      return Number(f.type === 5 || f.type === 14 ? BigInt.asIntN(32, n) : BigInt.asUintN(32, n))
    }
    throw Error('protobuf wire mismatch')
  }
  while (p < bytes.length) {
    const tag = Number(varint()), wire = tag & 7, number = tag >>> 3
    if (!number) throw Error('invalid protobuf tag')
    const f = fields.find(f => f.number === number)
    if (!f) {
      if (wire === 0) varint()
      else if (wire === 1) take(8)
      else if (wire === 2) take(Number(varint()))
      else if (wire === 5) take(4)
      else throw Error('unsupported protobuf wire')
      continue
    }
    if (f.oneof !== undefined) for (const other of fields) if (other.oneof === f.oneof) delete result[other.name]
    if (f.repeated && wire === 2 && ![9,11,12].includes(f.type)) {
      const size = Number(varint()), end = p + size
      if (end > bytes.length) throw Error('truncated packed protobuf')
      while (p < end) (result[f.name] as unknown[]).push(scalar(f, f.type === 1 ? 1 : 0))
      if (p !== end) throw Error('packed protobuf length mismatch')
    } else {
      const v = scalar(f, wire)
      if (f.map) { const entry = v as { key: string; value: unknown }; Object.defineProperty(result[f.name], entry.key, { value: entry.value, enumerable: true, configurable: true, writable: true }) }
      else if (f.repeated) (result[f.name] as unknown[]).push(v)
      else result[f.name] = v
    }
  }
  return result as T
}
export function toBase64(bytes: Uint8Array): string {
  let s = ''; for (const b of bytes) s += String.fromCharCode(b)
  return btoa(s)
}
export function fromBase64(s: string): Uint8Array {
  if (s.length > Math.ceil(MAX_BYTES / 3) * 4) throw Error('resource_exhausted: bridge message')
  return Uint8Array.from(atob(s), c => c.charCodeAt(0))
}
