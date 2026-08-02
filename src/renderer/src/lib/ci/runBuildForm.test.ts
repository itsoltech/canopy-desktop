import { describe, expect, it } from 'vitest'
import {
  initialFormValues,
  isCheckboxChecked,
  toggleCheckbox,
  multiValues,
  toggleMultiValue,
  missingRequired,
  toProperties,
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
