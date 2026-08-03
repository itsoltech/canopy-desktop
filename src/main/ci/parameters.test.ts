import { describe, expect, it } from 'vitest'
import { parseParameterSpec, parsePromptParameters } from './parameters'

describe('parseParameterSpec', () => {
  it('parses a prompt text parameter with label, description and validation', () => {
    expect(
      parseParameterSpec(
        "text description='Gdy zaznaczona opcja AppVersionUseCustom, uzupełnij to pole. Format: x.x.x' display='prompt' label='AppVersion' validationMode='not_empty'",
      ),
    ).toEqual({
      kind: 'text',
      display: 'prompt',
      label: 'AppVersion',
      description: 'Gdy zaznaczona opcja AppVersionUseCustom, uzupełnij to pole. Format: x.x.x',
      required: true,
      options: undefined,
      multiple: false,
      valueSeparator: ',',
      checkedValue: undefined,
      uncheckedValue: undefined,
    })
  })

  it('parses a checkbox with custom checked/unchecked values', () => {
    const spec = parseParameterSpec(
      "checkbox checkedValue='yes' uncheckedValue='no' display='prompt' label='Deploy'",
    )
    expect(spec.kind).toBe('checkbox')
    expect(spec.checkedValue).toBe('yes')
    expect(spec.uncheckedValue).toBe('no')
    expect(spec.required).toBe(false)
  })

  it('parses a single select with ordered data options', () => {
    const spec = parseParameterSpec(
      "select display='prompt' label='Environment' data_3='Prod' data_1='Test' data_2='Stage'",
    )
    expect(spec.kind).toBe('select')
    expect(spec.options).toEqual(['Test', 'Stage', 'Prod'])
    expect(spec.multiple).toBe(false)
  })

  it('parses a multi-select with its value separator', () => {
    const spec = parseParameterSpec(
      "select multiple='true' valueSeparator='|' display='prompt' data_1='dziekanat-desktop' data_2='kwestura-desktop'",
    )
    expect(spec.multiple).toBe(true)
    expect(spec.options).toEqual(['dziekanat-desktop', 'kwestura-desktop'])
    expect(spec.valueSeparator).toBe('|')
  })

  it('defaults the multi-select separator to a comma', () => {
    expect(parseParameterSpec("select multiple='true' data_1='a'").valueSeparator).toBe(',')
  })

  it('keeps the password kind so the dialog can mask it', () => {
    const spec = parseParameterSpec("password display='prompt' label='Deploy key'")
    expect(spec.kind).toBe('password')
    expect(spec.label).toBe('Deploy key')
  })

  it('unescapes doubled quotes and defaults unknown kinds to text', () => {
    const spec = parseParameterSpec("wildcard display='prompt' label='it''s odd'")
    expect(spec.kind).toBe('text')
    expect(spec.label).toBe("it's odd")
  })

  it('decodes TeamCity Unicode code-point escapes in labels, descriptions and options', () => {
    const spec = parseParameterSpec(
      "select display='prompt' label='Za[0x017C][0x00F3][0x0142][0x0107]' description='Uzupe[0x0142]nij pole [0x1F680]' data_1='Warto[0x015B][0x0107]'",
    )
    expect(spec.label).toBe('Zażółć')
    expect(spec.description).toBe('Uzupełnij pole 🚀')
    expect(spec.options).toEqual(['Wartość'])
  })

  it('decodes the pipe-prefixed Unicode escapes returned by older TeamCity servers', () => {
    const spec = parseParameterSpec(
      "text display='prompt' label='GakkoDatabase' description='Uzupe|0x0142nij baz|0x0119 dla |0x015brodowiska'",
    )
    expect(spec.description).toBe('Uzupełnij bazę dla środowiska')
  })

  it('leaves malformed or invalid Unicode escapes untouched', () => {
    const spec = parseParameterSpec(
      "text display='prompt' label='Bad [0xZZZZ] [0x110000] [0xD800]'",
    )
    expect(spec.label).toBe('Bad [0xZZZZ] [0x110000] [0xD800]')
  })

  it('handles empty/missing specs as plain hidden text', () => {
    expect(parseParameterSpec(undefined).display).toBe('normal')
    expect(parseParameterSpec('').kind).toBe('text')
  })
})

describe('parsePromptParameters', () => {
  it('keeps only prompt-displayed parameters, mapped with name and default value', () => {
    const params = parsePromptParameters({
      count: 3,
      property: [
        {
          name: 'AppVersion',
          value: '',
          type: { rawValue: "text display='prompt' label='AppVersion'" },
        },
        { name: 'internal.thing', value: 'x' },
        {
          name: 'Environment',
          value: 'Test',
          type: { rawValue: "select display='prompt' data_1='Test' data_2='Prod'" },
        },
      ],
    })
    expect(params.map((p) => p.name)).toEqual(['AppVersion', 'Environment'])
    expect(params[1].defaultValue).toBe('Test')
    expect(params[0].label).toBe('AppVersion')
  })

  it('never carries a password value into the form — the field starts empty', () => {
    const params = parsePromptParameters({
      property: [
        {
          name: 'DeployKey',
          value: 'zxx-scrambled-or-secret',
          type: { rawValue: "password display='prompt' label='Deploy key'" },
        },
      ],
    })
    expect(params[0].kind).toBe('password')
    expect(params[0].defaultValue).toBe('')
  })

  it('falls back to the parameter name when no label is given', () => {
    const params = parsePromptParameters({
      property: [
        { name: 'Affected', value: 'false', type: { rawValue: "checkbox display='prompt'" } },
      ],
    })
    expect(params[0].label).toBe('Affected')
  })
})
