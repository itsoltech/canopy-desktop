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

  it('falls back to the parameter name when no label is given', () => {
    const params = parsePromptParameters({
      property: [
        { name: 'Affected', value: 'false', type: { rawValue: "checkbox display='prompt'" } },
      ],
    })
    expect(params[0].label).toBe('Affected')
  })
})
