import 'react-native-get-random-values'

import { DarkTheme, DefaultTheme, ThemeProvider } from '@react-navigation/native'
import { Stack } from 'expo-router'
import * as Updates from 'expo-updates'
import React, { useCallback, useEffect, useState } from 'react'
import { Alert } from 'react-native'

import { AnimatedSplashOverlay } from '@/components/animated-icon'
import { AppConfig } from '@/config/app-config'
import { useColorScheme } from '@/hooks/use-color-scheme'
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
  return (
    <ThemeProvider value={colorScheme === 'dark' ? DarkTheme : DefaultTheme}>
      <EasUpdateAlert />
      <AnimatedSplashOverlay />
      {ready && (
        <Stack screenOptions={{ headerShown: false }}>
          <Stack.Screen name="(tabs)" />
          <Stack.Screen name="instance/[id]" options={{ headerShown: false }} />
          <Stack.Screen name="terminal" options={{ headerShown: false }} />
          <Stack.Screen name="settings/appearance" options={{ headerShown: false }} />
          <Stack.Screen
            name="scan"
            options={{
              presentation: 'modal',
              headerShown: false,
            }}
          />
        </Stack>
      )}
    </ThemeProvider>
  )
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
