import { describe, expect, it } from 'vitest'
import { parseWorkflowDispatch, validateWorkflowInputs } from './workflow'

describe('parseWorkflowDispatch', () => {
  it('parses workflow_dispatch from YAML 1.2 and keeps typed defaults', () => {
    const result = parseWorkflowDispatch(`
name: Release
on:
  workflow_dispatch:
    inputs:
      dry_run:
        description: Do not publish artifacts
        required: true
        type: boolean
        default: true
      channel:
        description: Release channel
        type: choice
        options: [next, stable]
        default: next
      environment:
        type: environment
        required: true
      notes:
        type: string
        default: ''
`)

    expect(result.isOk()).toBe(true)
    if (result.isErr()) throw result.error
    expect(result.value.name).toBe('Release')
    expect(result.value.inputs).toEqual([
      {
        name: 'dry_run',
        label: 'dry_run',
        description: 'Do not publish artifacts',
        required: true,
        type: 'boolean',
        defaultValue: true,
      },
      {
        name: 'channel',
        label: 'channel',
        description: 'Release channel',
        required: false,
        type: 'choice',
        defaultValue: 'next',
        options: ['next', 'stable'],
      },
      {
        name: 'environment',
        label: 'environment',
        description: undefined,
        required: true,
        type: 'environment',
        defaultValue: undefined,
      },
      {
        name: 'notes',
        label: 'notes',
        description: undefined,
        required: false,
        type: 'string',
        defaultValue: '',
      },
    ])
  })

  it('accepts a dispatch workflow without inputs', () => {
    const result = parseWorkflowDispatch('on: workflow_dispatch\n')
    expect(result.isOk() && result.value.inputs).toEqual([])
  })

  it('rejects workflows without workflow_dispatch and unsupported input types', () => {
    expect(parseWorkflowDispatch('on: [push]\n').isErr()).toBe(true)
    expect(
      parseWorkflowDispatch(`
on:
  workflow_dispatch:
    inputs:
      secret:
        type: password
`).isErr(),
    ).toBe(true)
  })

  it('rejects aliases, excessive depth and oversized documents', () => {
    expect(
      parseWorkflowDispatch(`
defaults: &defaults
  type: string
on:
  workflow_dispatch:
    inputs:
      value: *defaults
`).isErr(),
    ).toBe(true)

    const deep = `${'root:\n  '.repeat(45)}value\non: workflow_dispatch\n`
    expect(parseWorkflowDispatch(deep).isErr()).toBe(true)
    expect(parseWorkflowDispatch(`on: workflow_dispatch\n#${'x'.repeat(140_000)}`).isErr()).toBe(
      true,
    )
  })

  it('rejects malformed choices and invalid default types', () => {
    expect(
      parseWorkflowDispatch(`
on:
  workflow_dispatch:
    inputs:
      channel:
        type: choice
        options: []
`).isErr(),
    ).toBe(true)
    expect(
      parseWorkflowDispatch(`
on:
  workflow_dispatch:
    inputs:
      dry_run:
        type: boolean
        default: nope
`).isErr(),
    ).toBe(true)
  })

  it('validates typed dispatch inputs and rejects undeclared or missing values', () => {
    const parsed = parseWorkflowDispatch(`
on:
  workflow_dispatch:
    inputs:
      dry_run: { type: boolean, required: true }
      channel: { type: choice, options: [next, stable], required: true }
      notes: { type: string }
`)
    if (parsed.isErr()) throw parsed.error

    expect(validateWorkflowInputs(parsed.value, { dry_run: true, channel: 'next' }).isOk()).toBe(
      true,
    )
    expect(validateWorkflowInputs(parsed.value, { channel: 'next' }).isErr()).toBe(true)
    expect(validateWorkflowInputs(parsed.value, { dry_run: 'true', channel: 'next' }).isErr()).toBe(
      true,
    )
    expect(
      validateWorkflowInputs(parsed.value, {
        dry_run: true,
        channel: 'nightly',
        extra: 'nope',
      }).isErr(),
    ).toBe(true)
  })
})
