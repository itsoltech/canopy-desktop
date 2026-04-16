import type { TerminalThemeId } from '@/lib/storage/app-preferences-types'

export type TerminalPalette = {
  background: string
  foreground: string
  cursor: string
  cursorAccent: string
  selectionBackground: string
}

export type TerminalThemePreset = {
  id: TerminalThemeId
  label: string
  light: TerminalPalette
  dark: TerminalPalette
}

// "Canopy" — current inline palettes carried over verbatim so existing
// users see no visual change when the preference system ships with this as
// default.
const canopy: TerminalThemePreset = {
  id: 'canopy',
  label: 'Canopy',
  light: {
    background: '#ffffff',
    foreground: '#1d1d1f',
    cursor: '#1d1d1f',
    cursorAccent: '#ffffff',
    selectionBackground: '#b5d5ff',
  },
  dark: {
    background: '#000000',
    foreground: '#e6e6e6',
    cursor: '#e6e6e6',
    cursorAccent: '#000000',
    selectionBackground: '#3a3d41',
  },
}

// "One" — Atom One Light / One Dark.
const one: TerminalThemePreset = {
  id: 'one',
  label: 'One',
  light: {
    background: '#fafafa',
    foreground: '#383a42',
    cursor: '#526eff',
    cursorAccent: '#fafafa',
    selectionBackground: '#e5e5e6',
  },
  dark: {
    background: '#282c34',
    foreground: '#abb2bf',
    cursor: '#528bff',
    cursorAccent: '#282c34',
    selectionBackground: '#3e4451',
  },
}

export const TERMINAL_THEME_PRESETS: readonly TerminalThemePreset[] = [canopy, one]

export function resolveTerminalPalette(
  id: TerminalThemeId,
  mode: 'light' | 'dark',
): TerminalPalette {
  const preset = TERMINAL_THEME_PRESETS.find((p) => p.id === id) ?? canopy
  return mode === 'dark' ? preset.dark : preset.light
}
