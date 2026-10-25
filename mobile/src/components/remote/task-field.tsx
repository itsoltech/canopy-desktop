import React, { useState } from 'react'
import { ThemedText } from '@/components/themed-text'
import { Action, Field } from './common'
import { plain, valueOf } from '@/lib/remote/grpc/values'
import type { TaskField, Value } from '@/lib/remote/grpc/schema'
export function TaskFieldControl({ field, value, change }: { field: TaskField; value?: Value; change(value: Value): void }) {
  const current = plain(value ?? field.current_value ?? field.default_value), [json, setJson] = useState(JSON.stringify(current ?? null))
  if (field.read_only) return <ThemedText>{field.name}: {typeof current === 'string' ? current : JSON.stringify(current)} · {field.read_only_reason ?? 'Read only'}</ThemedText>
  if (field.choices.length) return <><ThemedText>{field.name}{field.required ? ' *' : ''}</ThemedText>{field.choices.map(choice => {
    const selected = field.multiple ? Array.isArray(current) && current.some(v => JSON.stringify(v) === JSON.stringify(plain(choice.value ?? valueOf(choice.id)))) : JSON.stringify(current) === JSON.stringify(plain(choice.value ?? valueOf(choice.id)))
    return <Action key={choice.id} title={`${selected ? '✓ ' : ''}${choice.label}`} run={() => {
      const next = choice.value ?? valueOf(choice.id)
      change(field.multiple ? valueOf(selected ? (current as unknown[]).filter(v => JSON.stringify(v) !== JSON.stringify(plain(next))) : [...(Array.isArray(current) ? current : []), plain(next)]) : next)
    }} />
  })}{!field.required && <Action title={`Clear ${field.name}`} run={() => change(valueOf(null))} />}</>
  if (field.kind === 'boolean' || typeof current === 'boolean') return <Action title={`${field.name}: ${current === true ? 'Yes' : 'No'}`} run={() => change(valueOf(current !== true))} />
  if (['number', 'integer', 'float'].includes(field.kind)) return <Field label={`${field.name}${field.required ? ' *' : ''}`} keyboardType="decimal-pad" value={current === null ? '' : String(current)} onChangeText={text => { if (text === '' || Number.isFinite(Number(text))) change(valueOf(text === '' ? null : Number(text))) }} />
  if (['string', 'text', 'date', 'datetime', 'duration'].includes(field.kind) || typeof current === 'string') return <Field label={`${field.name}${field.required ? ' *' : ''}`} multiline={field.kind === 'text'} value={String(current ?? '')} onChangeText={text => change(valueOf(text))} />
  return <><Field label={`${field.name} (structured value)`} multiline value={json} onChangeText={setJson} /><Action title={`Apply ${field.name}`} run={() => change(valueOf(JSON.parse(json)))} /></>
}
