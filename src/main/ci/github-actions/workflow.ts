import { err, ok, type Result } from 'neverthrow'
import { parseDocument, visit } from 'yaml'

const MAX_WORKFLOW_BYTES = 128 * 1024
const MAX_WORKFLOW_DEPTH = 40
const MAX_INPUTS = 25
const MAX_OPTIONS = 100
const MAX_TEXT = 1_000

export type GitHubWorkflowInputType = 'string' | 'boolean' | 'choice' | 'environment'

export interface GitHubWorkflowInput {
  name: string
  label: string
  description: string | undefined
  required: boolean
  type: GitHubWorkflowInputType
  defaultValue: string | boolean | undefined
  options?: string[]
}

export interface GitHubWorkflowSchema {
  name: string | undefined
  inputs: GitHubWorkflowInput[]
}

export interface GitHubWorkflowSchemaError {
  _tag: 'GitHubWorkflowSchemaInvalid'
  reason: string
}

function invalid(reason: string): Result<never, GitHubWorkflowSchemaError> {
  return err({ _tag: 'GitHubWorkflowSchemaInvalid', reason })
}

function record(value: unknown): Record<string, unknown> | undefined {
  if (!value || typeof value !== 'object' || Array.isArray(value)) return undefined
  return value as Record<string, unknown>
}

function dispatchDefinition(on: unknown): unknown | undefined {
  if (on === 'workflow_dispatch') return null
  if (Array.isArray(on)) return on.includes('workflow_dispatch') ? null : undefined
  return record(on)?.workflow_dispatch
}

function parseInput(
  name: string,
  raw: unknown,
): Result<GitHubWorkflowInput, GitHubWorkflowSchemaError> {
  const definition = record(raw)
  if (!definition) return invalid(`input ${name} must be an object`)
  const type = definition.type ?? 'string'
  if (type !== 'string' && type !== 'boolean' && type !== 'choice' && type !== 'environment') {
    return invalid(`input ${name} has unsupported type`)
  }
  if (definition.description != null && typeof definition.description !== 'string') {
    return invalid(`input ${name} has an invalid description`)
  }
  const description = definition.description?.slice(0, MAX_TEXT) as string | undefined
  const required = definition.required === true
  if (definition.required != null && typeof definition.required !== 'boolean') {
    return invalid(`input ${name} has an invalid required flag`)
  }

  let options: string[] | undefined
  if (type === 'choice') {
    if (
      !Array.isArray(definition.options) ||
      definition.options.length === 0 ||
      definition.options.length > MAX_OPTIONS ||
      definition.options.some((option) => typeof option !== 'string' || option.length > MAX_TEXT)
    ) {
      return invalid(`input ${name} has invalid choice options`)
    }
    options = definition.options
  } else if (definition.options != null) {
    return invalid(`input ${name} declares options for a non-choice type`)
  }

  const defaultValue = definition.default
  if (
    defaultValue != null &&
    ((type === 'boolean' && typeof defaultValue !== 'boolean') ||
      (type !== 'boolean' && typeof defaultValue !== 'string'))
  ) {
    return invalid(`input ${name} has an invalid default value`)
  }
  if (type === 'choice' && defaultValue != null && !options?.includes(defaultValue as string)) {
    return invalid(`input ${name} default is not one of its options`)
  }

  return ok({
    name,
    label: name,
    description,
    required,
    type,
    defaultValue: defaultValue as string | boolean | undefined,
    ...(options ? { options } : {}),
  })
}

/** Parse only the bounded subset of workflow YAML needed by workflow_dispatch. */
export function parseWorkflowDispatch(
  source: string,
): Result<GitHubWorkflowSchema, GitHubWorkflowSchemaError> {
  if (Buffer.byteLength(source, 'utf8') > MAX_WORKFLOW_BYTES) {
    return invalid('workflow file exceeds the size limit')
  }

  const document = parseDocument(source, {
    version: '1.2',
    strict: true,
    uniqueKeys: true,
    prettyErrors: false,
    customTags: [],
  })
  if (document.errors.length > 0) return invalid('workflow YAML is malformed')

  let unsafeReason: string | undefined
  visit(document, {
    Alias: () => {
      unsafeReason = 'workflow YAML aliases are not supported'
      return visit.BREAK
    },
    Node: (_key, _node, path) => {
      if (path.length > MAX_WORKFLOW_DEPTH) {
        unsafeReason = 'workflow YAML is nested too deeply'
        return visit.BREAK
      }
      return undefined
    },
  })
  if (unsafeReason) return invalid(unsafeReason)

  let parsed: unknown
  try {
    parsed = document.toJS({ maxAliasCount: 0 })
  } catch {
    return invalid('workflow YAML could not be decoded safely')
  }
  const workflow = record(parsed)
  if (!workflow) return invalid('workflow root must be an object')
  const dispatch = dispatchDefinition(workflow.on)
  if (dispatch === undefined) return invalid('workflow does not declare workflow_dispatch')
  if (dispatch == null) {
    return ok({ name: typeof workflow.name === 'string' ? workflow.name : undefined, inputs: [] })
  }
  const dispatchObject = record(dispatch)
  if (!dispatchObject) return invalid('workflow_dispatch must be an object')
  const rawInputs = dispatchObject.inputs
  if (rawInputs == null) {
    return ok({ name: typeof workflow.name === 'string' ? workflow.name : undefined, inputs: [] })
  }
  const inputs = record(rawInputs)
  if (!inputs) return invalid('workflow_dispatch inputs must be an object')
  const entries = Object.entries(inputs)
  if (entries.length > MAX_INPUTS) return invalid('workflow declares too many inputs')

  const parsedInputs: GitHubWorkflowInput[] = []
  for (const [name, value] of entries) {
    if (!/^[A-Za-z_][A-Za-z0-9_-]{0,99}$/.test(name)) {
      return invalid('workflow input has an invalid name')
    }
    const input = parseInput(name, value)
    if (input.isErr()) return err(input.error)
    parsedInputs.push(input.value)
  }
  return ok({
    name: typeof workflow.name === 'string' ? workflow.name.slice(0, MAX_TEXT) : undefined,
    inputs: parsedInputs,
  })
}

export function validateWorkflowInputs(
  schema: GitHubWorkflowSchema,
  values: Record<string, string | boolean>,
): Result<Record<string, string | boolean>, GitHubWorkflowSchemaError> {
  if (!values || typeof values !== 'object' || Array.isArray(values)) {
    return invalid('workflow inputs must be an object')
  }
  if (Object.keys(values).length > MAX_INPUTS || JSON.stringify(values).length > 60_000) {
    return invalid('workflow input payload exceeds the size limit')
  }
  const definitions = new Map(schema.inputs.map((input) => [input.name, input]))
  for (const name of Object.keys(values)) {
    if (!definitions.has(name)) return invalid(`workflow input ${name} is not declared`)
  }
  for (const input of schema.inputs) {
    const present = Object.prototype.hasOwnProperty.call(values, input.name)
    if (!present) {
      if (input.required && input.defaultValue === undefined) {
        return invalid(`workflow input ${input.name} is required`)
      }
      continue
    }
    const value = values[input.name]
    if (input.type === 'boolean') {
      if (typeof value !== 'boolean') return invalid(`workflow input ${input.name} must be boolean`)
      continue
    }
    if (typeof value !== 'string' || value.length > 10_000) {
      return invalid(`workflow input ${input.name} must be a bounded string`)
    }
    if (input.required && value.length === 0) {
      return invalid(`workflow input ${input.name} is required`)
    }
    if (input.type === 'choice' && !input.options?.includes(value)) {
      return invalid(`workflow input ${input.name} is not an allowed choice`)
    }
  }
  return ok(values)
}
