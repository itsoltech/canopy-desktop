export type AppThemeMode = 'light' | 'dark' | 'system'

export type TerminalThemeId = 'canopy' | 'one'

export type AppPreferences = {
  appTheme: AppThemeMode
  terminalThemeMode: AppThemeMode
  terminalThemeId: TerminalThemeId
}

export const DEFAULT_APP_PREFERENCES: AppPreferences = {
  appTheme: 'system',
  terminalThemeMode: 'system',
  terminalThemeId: 'canopy',
}
