export const MAX_DEVICE_NAME_LENGTH = 64

// eslint-disable-next-line no-control-regex
const CONTROL_CHARS = /[\u0000-\u001f\u007f-\u009f]/g
// Bidi embeddings/overrides/isolates and directional marks can make a name render as another one.
const BIDI_CONTROLS = /[\u200e\u200f\u202a-\u202e\u2066-\u2069]/g

/**
 * The device name arrives in the peer's unauthenticated pair frame (bounded only by the 256 KB
 * frame cap), is shown in the accept prompt, and is stored with a remembered device. Returns null
 * when nothing printable is left, so the caller can fall back to a generic label.
 */
export function sanitizeDeviceName(raw: unknown): string | null {
  if (typeof raw !== 'string') return null
  const name = raw
    .slice(0, MAX_DEVICE_NAME_LENGTH * 4)
    .replace(BIDI_CONTROLS, '')
    .replace(CONTROL_CHARS, ' ')
    .replace(/\s+/g, ' ')
    .trim()
  if (!name) return null
  // Array.from splits by code point, so the cut never leaves half a surrogate pair.
  return Array.from(name).slice(0, MAX_DEVICE_NAME_LENGTH).join('').trimEnd()
}
