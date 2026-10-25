import type { Value, Struct, TaskField } from './schema'
export function valueOf(value: unknown): Value {
  if (value === null || value === undefined) return { null_value: 0 }
  if (typeof value === 'string') return { string_value: value }
  if (typeof value === 'boolean') return { bool_value: value }
  if (typeof value === 'number') { if (!Number.isFinite(value)) throw Error('Finite number required'); return { number_value: value } }
  if (Array.isArray(value)) return { list_value: { values: value.map(valueOf) } }
  if (typeof value === 'object') return { struct_value: { fields: Object.fromEntries(Object.entries(value).map(([key, item]) => [key, valueOf(item)])) } }
  throw Error('Unsupported field value')
}
export function plain(value?: Value): unknown {
  if (!value || value.null_value !== undefined) return null
  if (value.string_value !== undefined) return value.string_value
  if (value.number_value !== undefined) return value.number_value
  if (value.bool_value !== undefined) return value.bool_value
  if (value.list_value) return value.list_value.values.map(plain)
  if (value.struct_value) return Object.fromEntries(Object.entries(value.struct_value.fields).map(([key, item]) => [key, plain(item)]))
  return null
}
export function validateFields(fields: TaskField[], values: Record<string, Value>): Struct {
  for (const field of fields) {
    const value = plain(values[field.id] ?? field.current_value ?? field.default_value)
    if (field.required && !field.read_only && (value === null || value === '' || Array.isArray(value) && !value.length)) throw Error(`${field.name} is required`)
  }
  const effective = { ...Object.fromEntries(fields.filter(f => f.required && !f.read_only && (f.current_value ?? f.default_value)).map(f => [f.id, (f.current_value ?? f.default_value)!])), ...values }
  return { fields: Object.fromEntries(Object.entries(effective).filter(([id]) => !fields.find(f => f.id === id)?.read_only)) }
}
