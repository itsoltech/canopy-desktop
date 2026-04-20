import 'react-native-get-random-values'

import { DarkTheme, DefaultTheme, ThemeProvider } from '@react-navigation/native'
import { Stack } from 'expo-router'
import { GestureHandlerRootView } from 'react-native-gesture-handler'
import * as Updates from 'expo-updates'
import React, { useCallback, useEffect, useState } from 'react'
import { Alert, AppState } from 'react-native'

import { AnimatedSplashOverlay } from '@/components/animated-icon'
import { AppConfig } from '@/config/app-config'
import { Colors } from '@/constants/theme'
import { useColorScheme } from '@/hooks/use-color-scheme'
import { reconnectIfDisconnected } from '@/lib/remote/session'
import { AppPreferencesStorage } from '@/lib/storage/app-preferences'

const CanopyDarkTheme = {
  ...DarkTheme,
  colors: {
    ...DarkTheme.colors,
    background: Colors.dark.background,
    card: Colors.dark.background,
  },
}

const CanopyLightTheme = {
  ...DefaultTheme,
  colors: {
    ...DefaultTheme.colors,
    background: Colors.light.background,
    card: Colors.light.background,
  },
}

function usePreferencesReady(): boolean {
  const [ready, setReady] = useState(AppPreferencesStorage.isLoaded())
  useEffect(() => {
    if (ready) return
    let cancelled = false
    void AppPreferencesStorage.whenLoaded().then(() => {
      if (!cancelled) setReady(true)
    })
    return () => {
      cancelled = true
    }
  }, [ready])
  return ready
}

export default function RootLayout(): React.ReactElement {
  const ready = usePreferencesReady()
  const colorScheme = useColorScheme()

  // AnimatedSplashOverlay, EasUpdateAlert, and AppStateReconnect are
  // intentionally outside the ready gate: they don't need themed context
  // and must be available immediately. ThemeProvider is gated on `ready`
  // so it never initialises React Navigation's native container with the
  // wrong background colour (the default appTheme is 'system', which
  // would produce a white background on a light-OS / dark-app device
  // until SecureStore finishes loading).
  return (
    <GestureHandlerRootView style={{ flex: 1 }}>
      <AnimatedSplashOverlay />
      <EasUpdateAlert />
      <AppStateReconnect />
      {ready && (
        <ThemeProvider value={colorScheme === 'dark' ? CanopyDarkTheme : CanopyLightTheme}>
          <Stack
            screenOptions={{
              headerShown: false,
              contentStyle: {
                backgroundColor:
                  colorScheme === 'dark' ? Colors.dark.background : Colors.light.background,
              },
            }}
          >
            <Stack.Screen name="(tabs)" />
            <Stack.Screen name="instance/[id]" options={{ headerShown: false }} />
            <Stack.Screen name="terminal" options={{ headerShown: false }} />
            <Stack.Screen name="settings/appearance" options={{ headerShown: false }} />
            <Stack.Screen
              name="worktree/new"
              options={{ presentation: 'modal', headerShown: false }}
            />
            <Stack.Screen
              name="project/new"
              options={{ presentation: 'modal', headerShown: false }}
            />
            <Stack.Screen
              name="scan"
              options={{
                presentation: 'modal',
                headerShown: false,
              }}
            />
          </Stack>
        </ThemeProvider>
      )}
    </GestureHandlerRootView>
  )
}

function AppStateReconnect(): null {
  useEffect(() => {
    const sub = AppState.addEventListener('change', (nextAppState) => {
      if (nextAppState === 'active') {
        reconnectIfDisconnected()
      }
    })
    return () => sub.remove()
  }, [])
  return null
}

function EasUpdateAlert(): null {
  const { isUpdatePending } = Updates.useUpdates()

  const check = useCallback(async () => {
    if (__DEV__) return
    try {
      const res = await Updates.checkForUpdateAsync()
      if (!res.isAvailable) return
      await Updates.fetchUpdateAsync()
    } catch {
      /* ignore — network errors are expected offline */
    }
  }, [])

  useEffect(() => {
    if (__DEV__) return

    if (!Updates.channel) {
      Updates.setExtraParamAsync('channel', AppConfig.CHANNEL).catch(() => {})
    }

    const int = setInterval(check, 60_000)
    check()
    return () => clearInterval(int)
  }, [check])

  useEffect(() => {
    if (!isUpdatePending) return

    Alert.alert('Update', 'A new version is available', [
      { text: 'Update', onPress: () => Updates.reloadAsync() },
      { text: 'Later', style: 'cancel', onPress: () => {} },
    ])
  }, [isUpdatePending])

  return null
}
