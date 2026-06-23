import 'react-native-get-random-values'

import { ThemeProvider } from 'expo-router/react-navigation'
import { Stack } from 'expo-router'
import { GestureHandlerRootView } from 'react-native-gesture-handler'
import * as Updates from 'expo-updates'
import React, { useCallback, useEffect, useState } from 'react'
import { Alert, AppState } from 'react-native'

import { AnimatedSplashOverlay } from '@/components/animated-icon'
import { AppConfig } from '@/config/app-config'
import { CanopyDarkTheme, CanopyLightTheme } from '@/constants/navigation-theme'
import { Colors } from '@/constants/theme'
import { useColorScheme } from '@/hooks/use-color-scheme'
import { reconnectIfDisconnected } from '@/lib/remote/session'
import { AppPreferencesStorage } from '@/lib/storage/app-preferences'

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
  const backgroundColor = colorScheme === 'dark' ? Colors.dark.background : Colors.light.background

  // AnimatedSplashOverlay, EasUpdateAlert, and AppStateReconnect are
  // intentionally outside the ready gate: they don't need themed context
  // and must be available immediately. ThemeProvider is gated on `ready`
  // so it never initialises React Navigation's native container with the
  // wrong background colour (the default appTheme is 'system', which
  // would produce a white background on a light-OS / dark-app device
  // until SecureStore finishes loading).
  return (
    <GestureHandlerRootView style={{ flex: 1, backgroundColor }}>
      <AnimatedSplashOverlay />
      <EasUpdateAlert />
      <AppStateReconnect />
      {ready && (
        <ThemeProvider value={colorScheme === 'dark' ? CanopyDarkTheme : CanopyLightTheme}>
          <Stack
            screenOptions={{
              contentStyle: { backgroundColor },
            }}
          >
            {/* Tabs own their per-tab native stacks/headers. */}
            <Stack.Screen name="(tabs)" options={{ headerShown: false }} />
            {/* Declare headerLargeTitle statically so the large-title mode is fixed from the
                first frame of the push transition — the dynamic title (nickname) is still set
                in-screen via <Stack.Screen>. Setting it only in-screen applies after mount, which
                reflows the content mid-transition. */}
            <Stack.Screen
              name="instance/[id]"
              options={{ headerLargeTitle: true, headerBackTitle: 'Instances' }}
            />
            {/* The instance nickname (the previous title) is too long for a stable
                back label on iOS — show just the chevron to avoid the show-then-hide flicker. */}
            <Stack.Screen name="terminal" options={{ headerBackButtonDisplayMode: 'minimal' }} />
            <Stack.Screen name="worktree/new" options={{ presentation: 'modal' }} />
            <Stack.Screen name="project/new" options={{ presentation: 'modal' }} />
            <Stack.Screen name="scan" options={{ presentation: 'modal', title: 'Scan' }} />
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
