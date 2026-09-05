import { describe, expect, it } from 'vitest'
import {
  changedProperties,
  initialFormValues,
  isCheckboxChecked,
  missingRequired,
  multiValues,
  toInputs,
  toProperties,
  toSubmittedInputs,
  toggleCheckbox,
  toggleMultiValue,
} from './runBuildForm'

function param(overrides: Partial<CiParameter>): CiParameter {
  return {
    name: 'P',
    kind: 'text',
    label: 'P',
    description: undefined,
    required: false,
    defaultValue: '',
    options: undefined,
    multiple: false,
    valueSeparator: ',',
    checkedValue: undefined,
    uncheckedValue: undefined,
    ...overrides,
  }
}

describe('initialFormValues', () => {
  it('prefills text and select fields with the parameter default', () => {
    const values = initialFormValues([
      param({ name: 'Environment', defaultValue: 'Test' }),
      param({ name: 'AppVersion' }),
    ])
    expect(values).toEqual({ Environment: 'Test', AppVersion: '' })
  })

  it('normalizes checkboxes to checked/unchecked values like the TeamCity dialog', () => {
    // Real-world shape (gakko): the checkbox VALUE carries a CLI fragment, and an
    // untouched unchecked checkbox must still submit uncheckedValue — submitting the
    // raw stored value ('') would drop the fragment entirely.
    const values = initialFormValues([
      param({
        name: 'Affected',
        kind: 'checkbox',
        defaultValue: '',
        checkedValue: '-Affected',
        uncheckedValue: '-Site %Sites%',
      }),
      param({ name: 'Deploy', kind: 'checkbox', defaultValue: '', checkedValue: '-Deploy' }),
      param({ name: 'On', kind: 'checkbox', defaultValue: 'true' }),
    ])
    expect(values).toEqual({ Affected: '-Site %Sites%', Deploy: '', On: 'true' })
  })
})

describe('checkbox helpers', () => {
  it('treats the checked value (default true) as checked and toggles both ways', () => {
    const plain = param({ kind: 'checkbox' })
    expect(isCheckboxChecked(plain, 'true')).toBe(true)
    expect(isCheckboxChecked(plain, '')).toBe(false)
    expect(toggleCheckbox(plain, '')).toBe('true')
    expect(toggleCheckbox(plain, 'true')).toBe('')

    const custom = param({ kind: 'checkbox', checkedValue: 'yes', uncheckedValue: 'no' })
    expect(isCheckboxChecked(custom, 'yes')).toBe(true)
    expect(toggleCheckbox(custom, 'yes')).toBe('no')
    expect(toggleCheckbox(custom, 'no')).toBe('yes')
  })
})

describe('multi-select helpers', () => {
  const sites = param({
    kind: 'select',
    multiple: true,
    options: ['a', 'b', 'c'],
    valueSeparator: ',',
  })

  it('splits and joins by the separator, dropping empties', () => {
    expect(multiValues(sites, 'a,c')).toEqual(['a', 'c'])
    expect(multiValues(sites, '')).toEqual([])
    expect(toggleMultiValue(sites, 'a,c', 'b')).toBe('a,c,b')
    expect(toggleMultiValue(sites, 'a,c,b', 'c')).toBe('a,b')
  })

  it('keeps option order stable when toggling everything on', () => {
    let v = ''
    for (const o of ['c', 'a', 'b']) v = toggleMultiValue(sites, v, o)
    expect(multiValues(sites, v)).toEqual(['c', 'a', 'b'])
  })
})

describe('missingRequired', () => {
  it('lists required parameters whose value is blank', () => {
    const params = [
      param({ name: 'AppVersion', required: true }),
      param({ name: 'Environment', required: true, defaultValue: 'Test' }),
      param({ name: 'Optional' }),
    ]
    expect(
      missingRequired(params, { AppVersion: '  ', Environment: 'Test', Optional: '' }),
    ).toEqual(['AppVersion'])
    expect(
      missingRequired(params, { AppVersion: '1.2.3', Environment: 'Test', Optional: '' }),
    ).toEqual([])
  })

  it('allows an explicit empty workflow default to satisfy a required input', () => {
    const parameter = param({ name: 'notes', required: true, hasDefault: true })

    expect(missingRequired([parameter], { notes: '' })).toEqual([])
    expect(toInputs([parameter], { notes: '' })).toEqual({})
  })
})

describe('toProperties', () => {
  it('maps every parameter to a name/value pair from the form state', () => {
    const params = [param({ name: 'A' }), param({ name: 'B' })]
    expect(toProperties(params, { A: '1', B: '' })).toEqual([
      { name: 'A', value: '1' },
      { name: 'B', value: '' },
    ])
  })

  it('omits untouched password prompts and keeps typed ones', () => {
    // A property present in the payload OVERRIDES the configuration's stored value —
    // an empty password must be left out so TeamCity falls back to its own secret.
    const params = [param({ name: 'DeployKey', kind: 'password' }), param({ name: 'Env' })]
    expect(toProperties(params, { DeployKey: '', Env: 'Test' })).toEqual([
      { name: 'Env', value: 'Test' },
    ])
    expect(toProperties(params, { DeployKey: 'typed-secret', Env: 'Test' })).toEqual([
      { name: 'DeployKey', value: 'typed-secret' },
      { name: 'Env', value: 'Test' },
    ])
  })
})

describe('toInputs', () => {
  it('keeps GitHub boolean values typed and omits blank optional strings', () => {
    const params = [
      param({
        name: 'dry_run',
        kind: 'checkbox',
        valueType: 'boolean',
        checkedValue: 'true',
        uncheckedValue: 'false',
      }),
      param({ name: 'notes', valueType: 'string' }),
      param({ name: 'channel', kind: 'select', valueType: 'string', required: true }),
    ]
    expect(toInputs(params, { dry_run: 'true', notes: '', channel: 'next' })).toEqual({
      dry_run: true,
      channel: 'next',
    })
    expect(toInputs(params, { dry_run: 'false', notes: 'safe', channel: 'next' })).toEqual({
      dry_run: false,
      notes: 'safe',
      channel: 'next',
    })
  })

  it('uses the exact dispatched input set for confirmation rows', () => {
    const params = [
      param({
        name: 'dry_run',
        kind: 'checkbox',
        valueType: 'boolean',
        checkedValue: 'true',
        uncheckedValue: 'false',
      }),
      param({ name: 'environment', kind: 'select', hasDefault: true, defaultValue: 'staging' }),
      param({ name: 'notes' }),
    ]
    const values = { dry_run: 'false', environment: '', notes: '' }

    expect(toSubmittedInputs(params, values)).toEqual([{ name: 'dry_run', value: 'false' }])
    expect(
      Object.fromEntries(toSubmittedInputs(params, values).map((row) => [row.name, row.value])),
    ).toEqual(
      Object.fromEntries(Object.entries(toInputs(params, values)).map(([k, v]) => [k, String(v)])),
    )
  })
})

describe('changedProperties', () => {
  const param = (over: Partial<CiParameter> & { name: string }): CiParameter => ({
    kind: 'text',
    label: over.name,
    description: undefined,
    required: false,
    defaultValue: '',
    options: undefined,
    multiple: false,
    valueSeparator: ',',
    checkedValue: undefined,
    uncheckedValue: undefined,
    ...over,
  })

  it('drops everything left at its default', () => {
    const params = [param({ name: 'Deploy' }), param({ name: 'Env', defaultValue: 'Test' })]
    expect(
      changedProperties(params, [
        { name: 'Deploy', value: '' },
        { name: 'Env', value: 'Test' },
      ]),
    ).toEqual([])
  })

  it('compares checkboxes against their normalized checked and unchecked defaults', () => {
    const params = [
      param({
        name: 'Affected',
        kind: 'checkbox',
        defaultValue: '',
        checkedValue: '-Affected',
        uncheckedValue: '-Site %Sites%',
      }),
      param({ name: 'Deploy', kind: 'checkbox', defaultValue: '', checkedValue: '-Deploy' }),
    ]

    expect(
      changedProperties(params, [
        { name: 'Affected', value: '-Site %Sites%' },
        { name: 'Deploy', value: '' },
      ]),
    ).toEqual([])
    expect(changedProperties(params, [{ name: 'Deploy', value: '-Deploy' }])).toEqual([
      { name: 'Deploy', value: '-Deploy' },
    ])
  })

  it('keeps what differs, whatever the parameter is called', () => {
    const params = [param({ name: 'TARGET_ENV', defaultValue: 'staging' })]
    expect(changedProperties(params, [{ name: 'TARGET_ENV', value: 'prod' }])).toEqual([
      { name: 'TARGET_ENV', value: 'prod' },
    ])
  })

  it('treats an unknown parameter as changed', () => {
    expect(changedProperties([], [{ name: 'Rogue', value: 'x' }])).toEqual([
      { name: 'Rogue', value: 'x' },
    ])
  })

  it('never echoes a password value', () => {
    const params = [param({ name: 'DbPassword', kind: 'password' })]
    expect(changedProperties(params, [{ name: 'DbPassword', value: 'hunter2' }])).toEqual([
      { name: 'DbPassword', value: '********' },
    ])
  })
})
