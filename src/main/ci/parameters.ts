// TeamCity "typed parameter" spec parsing — the `type.rawValue` string behind the
// fields TeamCity renders in its own "Run custom build" dialog, e.g.:
//   text display='prompt' label='AppVersion' validationMode='not_empty'
//   checkbox checkedValue='yes' uncheckedValue='no' display='prompt'
//   select multiple='true' data_1='Test' data_2='Prod' display='prompt'
// Attribute values are single-quoted with `''` escaping a literal quote.

import type { CiParameter } from './types'

interface ParsedSpec {
  kind: 'text' | 'password' | 'checkbox' | 'select'
  display: 'prompt' | 'normal' | 'hidden'
  label: string | undefined
  description: string | undefined
  required: boolean
  options: string[] | undefined
  multiple: boolean
  valueSeparator: string
  checkedValue: string | undefined
  uncheckedValue: string | undefined
}

const ATTR_RE = /([A-Za-z_][A-Za-z0-9_]*)='((?:[^']|'')*)'/g

function decodeCodePoint(token: string, hex: string): string {
  const codePoint = Number.parseInt(hex, 16)
  const unsafeControl = codePoint <= 0x1f || (codePoint >= 0x7f && codePoint <= 0x9f)
  const bidiOverride =
    codePoint === 0x061c ||
    codePoint === 0x200e ||
    codePoint === 0x200f ||
    (codePoint >= 0x202a && codePoint <= 0x202e) ||
    (codePoint >= 0x2066 && codePoint <= 0x2069)
  if (
    codePoint > 0x10ffff ||
    (codePoint >= 0xd800 && codePoint <= 0xdfff) ||
    unsafeControl ||
    bidiOverride
  )
    return token
  return String.fromCodePoint(codePoint)
}

/** TeamCity escapes non-ASCII parameter metadata as `[0xNNNN]` or legacy `|0xNNNN`. */
export function decodeParameterText(value: string): string {
  return value
    .replace(/\[0x([0-9A-Fa-f]{4,6})\]/g, decodeCodePoint)
    .replace(/\|0x([0-9A-Fa-f]{4})/g, decodeCodePoint)
}

export function parseParameterSpec(rawValue: string | undefined): ParsedSpec {
  const raw = rawValue ?? ''
  const kindToken = raw.trim().split(/\s+/, 1)[0] ?? ''
  // Unknown kinds degrade to a plain text input; `password` keeps its own kind so
  // the dialog can MASK it — TeamCity prompts for secrets in deploy configurations.
  const kind =
    kindToken === 'checkbox' || kindToken === 'select' || kindToken === 'password'
      ? kindToken
      : ('text' as ParsedSpec['kind'])

  const attrs: Record<string, string> = {}
  const data: Array<{ index: number; value: string }> = []
  for (const m of raw.matchAll(ATTR_RE)) {
    const name = m[1]
    const value = decodeParameterText(m[2].replaceAll("''", "'"))
    const dataMatch = /^data_(\d+)$/.exec(name)
    if (dataMatch) data.push({ index: Number(dataMatch[1]), value })
    else attrs[name] = value
  }
  data.sort((a, b) => a.index - b.index)

  const display =
    attrs.display === 'prompt' || attrs.display === 'hidden' ? attrs.display : 'normal'

  return {
    kind,
    display,
    label: attrs.label,
    description: attrs.description,
    required: attrs.validationMode === 'not_empty',
    options: kind === 'select' ? data.map((d) => d.value) : undefined,
    multiple: attrs.multiple === 'true',
    valueSeparator: attrs.valueSeparator || ',',
    checkedValue: kind === 'checkbox' ? attrs.checkedValue : undefined,
    uncheckedValue: kind === 'checkbox' ? attrs.uncheckedValue : undefined,
  }
}

interface RawProperty {
  name: string
  value?: string
  type?: { rawValue?: string }
}

/**
 * The parameters TeamCity would prompt for in its "Run custom build" dialog:
 * everything with `display='prompt'`, in server order, with the property's current
 * value as the form default.
 */
export function parsePromptParameters(json: {
  count?: number
  property?: RawProperty[]
}): CiParameter[] {
  return (json.property ?? []).flatMap((prop) => {
    const spec = parseParameterSpec(prop.type?.rawValue)
    if (spec.display !== 'prompt') return []
    return [
      {
        name: prop.name,
        kind: spec.kind,
        label: spec.label || prop.name,
        description: spec.description,
        required: spec.required,
        // Password prompts start EMPTY, like TeamCity's own dialog: `prop.value` is
        // either the secret (must not enter the renderer) or a scrambled placeholder
        // (would be queued as a literal value). `toProperties` then OMITS a field
        // left blank rather than sending '', so TeamCity uses its stored value —
        // a property present in the payload would override it.
        defaultValue: spec.kind === 'password' ? '' : (prop.value ?? ''),
        options: spec.options,
        multiple: spec.multiple,
        valueSeparator: spec.valueSeparator,
        checkedValue: spec.checkedValue,
        uncheckedValue: spec.uncheckedValue,
      },
    ]
  })
}
