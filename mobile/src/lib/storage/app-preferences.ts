import * as SecureStore from 'expo-secure-store'

import {
  type AppPreferences,
  DEFAULT_APP_PREFERENCES,
  type AppThemeMode,
  type TerminalThemeId,
} from './app-preferences-types'

const STORAGE_KEY = 'canopy.appPreferences.v1'

let current: AppPreferences = DEFAULT_APP_PREFERENCES
let loaded = false
const listeners = new Set<() => void>()

function notify(): void {
  for (const fn of listeners) fn()
}

// Eager load at module import so first render usually sees the persisted
// value. If SecureStore is slow on cold boot, `whenLoaded()` lets the root
// layout gate its Stack mount until hydration completes.
const loadPromise: Promise<void> = SecureStore.getItemAsync(STORAGE_KEY)
  .then((raw) => {
    if (!raw) return
    try {
      const parsed = JSON.parse(raw) as unknown
      if (parsed && typeof parsed === 'object') {
        const obj = parsed as Record<string, unknown>
        const appTheme = isAppThemeMode(obj.appTheme)
          ? obj.appTheme
          : DEFAULT_APP_PREFERENCES.appTheme
        const terminalThemeId = isTerminalThemeId(obj.terminalThemeId)
          ? obj.terminalThemeId
          : DEFAULT_APP_PREFERENCES.terminalThemeId
        current = { appTheme, terminalThemeId }
      }
    } catch {
      // keep defaults on parse error
    }
  })
  .catch(() => {
    // SecureStore unavailable — keep defaults
  })
  .finally(() => {
    loaded = true
    notify()
  })

export const AppPreferencesStorage = {
  get(): AppPreferences {
    return current
  },
  isLoaded(): boolean {
    return loaded
  },
  whenLoaded(): Promise<void> {
    return loadPromise
  },
  async set(patch: Partial<AppPreferences>): Promise<void> {
    current = { ...current, ...patch }
    await SecureStore.setItemAsync(STORAGE_KEY, JSON.stringify(current))
    notify()
  },
  subscribe(fn: () => void): () => void {
    listeners.add(fn)
    return () => {
      listeners.delete(fn)
    }
  },
}

function isAppThemeMode(v: unknown): v is AppThemeMode {
  return v === 'light' || v === 'dark' || v === 'system'
}

function isTerminalThemeId(v: unknown): v is TerminalThemeId {
  return v === 'canopy' || v === 'one'
}
