import { useCallback, useEffect, useState } from 'react'

import { SavedInstancesStorage } from '@/lib/storage/saved-instances'
import type { SavedInstance } from '@/lib/storage/saved-instances-types'

type UseSavedInstancesResult = {
  instances: SavedInstance[] | null
  loading: boolean
  error: Error | null
  refresh: () => Promise<void>
}

export function useSavedInstances(): UseSavedInstancesResult {
  const [instances, setInstances] = useState<SavedInstance[] | null>(null)
  const [error, setError] = useState<Error | null>(null)

  const refresh = useCallback(async () => {
    try {
      const list = await SavedInstancesStorage.list()
      setInstances(list)
      setError(null)
    } catch (e) {
      setError(e instanceof Error ? e : new Error(String(e)))
      setInstances([])
    }
  }, [])

  useEffect(() => {
    refresh()
    const unsub = SavedInstancesStorage.subscribe(refresh)
    return unsub
  }, [refresh])

  return {
    instances,
    loading: instances === null,
    error,
    refresh,
  }
}

export function useSavedInstance(id: string | undefined): {
  instance: SavedInstance | null
  loading: boolean
} {
  const { instances, loading } = useSavedInstances()
  if (!id) return { instance: null, loading }
  const instance = instances?.find((i) => i.id === id) ?? null
  return { instance, loading }
}
