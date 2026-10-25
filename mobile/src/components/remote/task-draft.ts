import { useState } from 'react'
import type { HostStore } from '@/lib/remote/grpc/persistence'
// Every setter captures the original target. Navigation never redirects pending writes.
export function useTaskDraft<T>(store: HostStore | undefined, target: string, field: string, initial: T, failure: (message: string) => void): [T, (value: T | ((current: T) => T)) => void] {
  const stored = () => (store?.snapshot().taskDrafts?.[target]?.[field] as T | undefined) ?? initial
  const [local, setLocal] = useState<{ target: string; store?: HostStore; value: T }>(() => ({ target, store, value: stored() }))
  const value = local.target === target && local.store === store ? local.value : stored()
  return [value, update => {
    const next = typeof update === 'function' ? (update as (current: T) => T)(value) : update
    setLocal({ target, store, value: next })
    if (store) void store.update(state => { ((state.taskDrafts ??= {})[target] ??= {})[field] = next; return null }).catch(error => failure(String(error)))
  }]
}
