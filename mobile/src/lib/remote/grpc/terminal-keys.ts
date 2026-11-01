import { TerminalInputMode as Mode } from './schema'
export type KeyModifiers = { shift?: boolean; alt?: boolean; ctrl?: boolean; meta?: boolean }
export function terminalKey(key: string, modes: bigint, modifiers: KeyModifiers = {}): string {
  const { shift, alt, ctrl, meta } = modifiers
  const modifier = 1 + (shift ? 1 : 0) + (alt ? 2 : 0) + (ctrl ? 4 : 0) + (meta ? 8 : 0)
  const cursor: Record<string, string> = { up: 'A', down: 'B', right: 'C', left: 'D', home: 'H', end: 'F' }
  if (cursor[key]) return modifier > 1 ? `\x1b[1;${modifier}${cursor[key]}` : `\x1b${modes & BigInt(Mode.TERMINAL_INPUT_MODE_APPLICATION_CURSOR) ? 'O' : '['}${cursor[key]}`
  const tilde: Record<string, number> = { insert: 2, delete: 3, pageup: 5, pagedown: 6, f5: 15, f6: 17, f7: 18, f8: 19, f9: 20, f10: 21, f11: 23, f12: 24 }
  if (tilde[key]) return `\x1b[${tilde[key]}${modifier > 1 ? `;${modifier}` : ''}~`
  const fn: Record<string, string> = { f1: 'P', f2: 'Q', f3: 'R', f4: 'S' }
  if (fn[key]) return modifier > 1 ? `\x1b[1;${modifier}${fn[key]}` : `\x1bO${fn[key]}`
  let value = ({ escape: '\x1b', backspace: ctrl ? '\x08' : '\x7f', tab: shift ? '\x1b[Z' : '\t', enter: modes & BigInt(Mode.TERMINAL_INPUT_MODE_NEW_LINE_MODE) ? '\r\n' : '\r' } as Record<string, string>)[key]
  if (value === undefined) {
    if (key.length !== 1) throw Error('Unsupported terminal key')
    value = shift ? key.toUpperCase() : key
    if (ctrl) {
      const code = value.toUpperCase().charCodeAt(0)
      if (value === ' ' || value === '2') value = '\x00'
      else if (value === '?' || value === '8') value = '\x7f'
      else if (code >= 64 && code <= 95) value = String.fromCharCode(code & 31)
      else throw Error('Unsupported control key')
    }
  }
  return (alt || meta ? '\x1b' : '') + value
}
