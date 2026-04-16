import { useSyncExternalStore } from 'react'

import { AppPreferencesStorage } from '@/lib/storage/app-preferences'
import type { AppPreferences } from '@/lib/storage/app-preferences-types'

export function useAppPreferences(): AppPreferences {
  return useSyncExternalStore(
    AppPreferencesStorage.subscribe,
    AppPreferencesStorage.get,
    AppPreferencesStorage.get,
  )
}
