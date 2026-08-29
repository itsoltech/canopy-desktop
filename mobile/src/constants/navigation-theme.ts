import { DarkTheme, DefaultTheme } from 'expo-router/react-navigation'

import { Colors } from './theme'

// React Navigation themes mapped to the Canopy palette. `card`/`text` drive the
// native header (and NativeTabs bar) colors; `primary` is the tint (back chevron,
// bar buttons, selected tab) — kept monochrome (= text) to match the app's look.
// Shared so the root layout AND the NativeTabs layout use the same theme — the
// tab bar needs its own ThemeProvider or it flashes the default (light) theme
// during transitions on iOS 26.
export const CanopyDarkTheme = {
  ...DarkTheme,
  colors: {
    ...DarkTheme.colors,
    background: Colors.dark.background,
    card: Colors.dark.background,
    text: Colors.dark.text,
    primary: Colors.dark.text,
    border: 'rgba(127, 127, 127, 0.2)',
  },
}

export const CanopyLightTheme = {
  ...DefaultTheme,
  colors: {
    ...DefaultTheme.colors,
    background: Colors.light.background,
    card: Colors.light.background,
    text: Colors.light.text,
    primary: Colors.light.text,
    border: 'rgba(127, 127, 127, 0.2)',
  },
}
