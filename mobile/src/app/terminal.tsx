import { useLocalSearchParams, useRouter } from 'expo-router'
import { SymbolView } from 'expo-symbols'
import { useCallback, useEffect, useMemo, useRef, useState } from 'react'
import { Alert, Keyboard, Pressable, StyleSheet, View } from 'react-native'
import { SafeAreaView } from 'react-native-safe-area-context'

import { TerminalTabBar } from '@/components/terminal/tab-bar'
import TerminalView, { type TerminalViewHandle } from '@/components/terminal/terminal-view'
import { ToolPickerSheet } from '@/components/terminal/tool-picker-sheet'
import { ThemedText } from '@/components/themed-text'
import { ThemedView } from '@/components/themed-view'
import { resolveTerminalPalette } from '@/constants/terminal-themes'
import { Spacing } from '@/constants/theme'
import { useAppPreferences } from '@/hooks/use-app-preferences'
import { useTerminalColorScheme } from '@/hooks/use-color-scheme'
import { useRemoteSession } from '@/hooks/use-remote-session'
import {
  useActiveTabId,
  useIsHydrated,
  useProfiles,
  useTabsFor,
  useTools,
} from '@/hooks/use-remote-state'
import { useSavedInstance } from '@/hooks/use-saved-instances'
import { useTheme } from '@/hooks/use-theme'

export default function TerminalScreen(): React.ReactElement {
  const { instanceId, worktreePath: encodedPath } = useLocalSearchParams<{
    instanceId: string
    worktreePath: string
  }>()
  const router = useRouter()
  const theme = useTheme()
  const colorScheme = useTerminalColorScheme()
  const { instance } = useSavedInstance(instanceId)
  const { state: sessionState } = useRemoteSession()
  const { terminalThemeId } = useAppPreferences()

  const worktreePath = useMemo(() => {
    if (!encodedPath) return ''
    try {
      return decodeURIComponent(encodedPath)
    } catch {
      return encodedPath
    }
  }, [encodedPath])

  const tabs = useTabsFor(worktreePath)
  const remoteActiveTabId = useActiveTabId(worktreePath)
  const [localTabId, setLocalTabId] = useState<string | null>(null)
  const activeTabId = localTabId ?? remoteActiveTabId ?? tabs[0]?.id ?? null
  const activeTab = tabs.find((t) => t.id === activeTabId) ?? null

  const tools = useTools()
  const profiles = useProfiles()
  const hydrated = useIsHydrated()
  const [pickerVisible, setPickerVisible] = useState(false)

  const themeMode: 'light' | 'dark' = colorScheme === 'dark' ? 'dark' : 'light'
  const slotBackground = resolveTerminalPalette(terminalThemeId, themeMode).background
  const title = deriveWorktreeLabel(worktreePath)

  const api = sessionState.kind === 'ready' ? sessionState.api : null
  const sessionId = activeTab?.focusedSessionId ?? null

  const terminalRef = useRef<TerminalViewHandle>(null)
  const writeQueueRef = useRef<string[]>([])
  const flushTimerRef = useRef<ReturnType<typeof setTimeout> | null>(null)
  const [streamError, setStreamError] = useState<string | null>(null)

  // Mirror latest api/sessionId into refs so the onInput/onResize callbacks
  // passed to the DOM component can stay referentially stable forever. If
  // they changed on every tab switch, TerminalView's internal
  // `useEffect([onInput, onResize, ...])` would fire, rebuild the xterm
  // instance, and we'd be back in the stale-Proxy trap (see below).
  const apiRef = useRef(api)
  apiRef.current = api
  const sessionIdRef = useRef(sessionId)
  sessionIdRef.current = sessionId

  // The Expo DOM-component ref is a Proxy that only resolves imperative
  // methods after the WebView finishes loading and the inner xterm posts
  // REGISTER_DOM_IMPERATIVE_HANDLE_PROPS back to native. Before that,
  // `ref.current.write` is `undefined`. PTY chunks can start arriving
  // before the webview is ready, so we retry until the method materializes.
  const scheduleFlush = useCallback((): void => {
    if (flushTimerRef.current !== null) return
    const tick = (): void => {
      flushTimerRef.current = null
      const chunks = writeQueueRef.current
      if (chunks.length === 0) return
      const write = terminalRef.current?.write
      if (typeof write !== 'function') {
        // Terminal handle not armed yet — hold chunks and retry soon.
        flushTimerRef.current = setTimeout(tick, 50)
        return
      }
      writeQueueRef.current = []
      write(chunks.join(''))
    }
    flushTimerRef.current = setTimeout(tick, 16)
  }, [])

  // Subscribe to PTY output + resize events for the active session.
  useEffect(() => {
    if (!api || !sessionId) return
    setStreamError(null)

    // Blank the terminal before the host's replay buffer lands. Otherwise
    // the new session's scrollback would be appended after the previous
    // tab's leftover content.
    const clear = terminalRef.current?.clear
    if (typeof clear === 'function') clear()

    const unsubscribers: Array<() => void> = []

    unsubscribers.push(
      api.subscribe<string>(`pty.data.${sessionId}`, (chunk) => {
        if (typeof chunk !== 'string') return
        writeQueueRef.current.push(chunk)
        scheduleFlush()
      }),
    )
    unsubscribers.push(
      api.subscribe<{ cols: number; rows: number } | null>(`pty.resized.${sessionId}`, (dims) => {
        if (!dims) return
        const resize = terminalRef.current?.resize
        if (typeof resize === 'function') resize(dims.cols, dims.rows)
      }),
    )
    unsubscribers.push(
      api.subscribe(`pty.closed.${sessionId}`, () => {
        setStreamError('Session ended')
      }),
    )

    let cancelled = false
    void api.pty.subscribe(sessionId).catch((err) => {
      if (cancelled) return
      const msg = err instanceof Error ? err.message : String(err)
      setStreamError(msg)
    })

    // After the new session's replay lands (which includes a pty.resized
    // echoing the session's stored dims), re-fit xterm to the current host
    // height and push those dims to the remote PTY. Without this, switching
    // tabs while the iOS keyboard is open leaves the new session sized for
    // the session's previous (keyboard-closed) viewport, so the bottom rows
    // end up hidden under the keyboard. The short delay lets the replay
    // events settle before we correct — otherwise a late pty.resized would
    // clobber our fit right after it ran.
    const refitTimer = setTimeout(() => {
      if (cancelled) return
      const refit = terminalRef.current?.refit
      if (typeof refit === 'function') refit()
    }, 120)

    return () => {
      cancelled = true
      clearTimeout(refitTimer)
      for (const un of unsubscribers) {
        try {
          un()
        } catch {
          /* ignore */
        }
      }
      // Drop any pending chunks — they belong to the session we're leaving.
      writeQueueRef.current = []
      if (flushTimerRef.current !== null) {
        clearTimeout(flushTimerRef.current)
        flushTimerRef.current = null
      }
      void api.pty.unsubscribe(sessionId).catch(() => {
        /* ignore */
      })
    }
  }, [api, sessionId, scheduleFlush])

  // Stable forever — read api/sessionId from refs so switching tabs doesn't
  // invalidate these callbacks and rebuild the xterm via terminal-view's
  // internal useEffect.
  const onInput = useCallback(async (data: string) => {
    const api = apiRef.current
    const sid = sessionIdRef.current
    if (!api || !sid) return
    try {
      await api.pty.write(sid, data)
    } catch (err) {
      const msg = err instanceof Error ? err.message : String(err)
      if (/ActionRejected|rejected/i.test(msg)) {
        setStreamError('Approve terminal input on desktop to continue')
      }
    }
  }, [])

  const onResize = useCallback(async (cols: number, rows: number) => {
    const api = apiRef.current
    const sid = sessionIdRef.current
    if (!api || !sid) return
    try {
      await api.pty.resize(sid, cols, rows)
    } catch {
      /* ignore — not fatal, dimensions will drift */
    }
  }, [])

  const handleTabSelect = useCallback(
    (tabId: string) => {
      setLocalTabId(tabId)
      if (api) {
        void api.tabs.activate(tabId).catch(() => {
          /* ignore */
        })
      }
    },
    [api],
  )

  const handleToolPick = useCallback(
    async (toolId: string, profileId?: string) => {
      setPickerVisible(false)
      if (!api) return
      try {
        const result = await api.tools.spawn(toolId, worktreePath, profileId)
        setLocalTabId(result.tabId)
        void api.tabs.activate(result.tabId).catch(() => {
          /* ignore */
        })
      } catch (err) {
        const msg = err instanceof Error ? err.message : String(err)
        setStreamError(msg)
      }
    },
    [api, worktreePath],
  )

  const handleTabLongPress = useCallback(
    (tabId: string) => {
      if (!api) return
      const tab = tabs.find((t) => t.id === tabId)
      const label = tab?.name ?? 'Tab'
      Alert.alert(label, undefined, [
        { text: 'Cancel', style: 'cancel' },
        {
          text: 'Close tab',
          style: 'destructive',
          onPress: () => {
            // Drop any local override pointing at the tab we're about to
            // close so the `activeTabId` fallback (`remoteActiveTabId ??
            // tabs[0]?.id`) can pick up whatever the host chooses next.
            setLocalTabId((current) => (current === tabId ? null : current))
            void api.tabs.close(tabId).catch((err: unknown) => {
              const msg = err instanceof Error ? err.message : String(err)
              setStreamError(msg)
            })
          },
        },
      ])
    },
    [api, tabs],
  )

  // Dismiss the soft keyboard when the user taps anywhere in the header
  // (back button area, title, or empty space around them). We try two
  // paths: Keyboard.dismiss() resigns the native WKWebView's first
  // responder, and the DOM handle's blur() unfocuses xterm's textarea
  // from inside the webview. Either one alone is usually enough, but
  // combining them is resilient to iOS quirks where the webview keeps
  // the keyboard up until both sides agree focus is gone.
  const dismissKeyboard = useCallback((): void => {
    Keyboard.dismiss()
    const blur = terminalRef.current?.blur
    if (typeof blur === 'function') blur()
  }, [])

  return (
    <ThemedView style={styles.container}>
      <SafeAreaView edges={['top']} style={styles.safeArea}>
        <View
          style={styles.header}
          // Observe touches without stealing the responder from the back
          // button. Returning false keeps children (the Pressable) eligible
          // to claim the gesture, but the capture callback still runs —
          // which is where we dismiss the keyboard on every tap inside the
          // header strip (back button, title area, or the gap between).
          onStartShouldSetResponderCapture={(): boolean => {
            dismissKeyboard()
            return false
          }}
        >
          <Pressable
            onPress={() => router.back()}
            style={({ pressed }) => [styles.iconBack, pressed && styles.pressed]}
            accessibilityLabel="Back"
          >
            <SymbolView
              name={{ ios: 'chevron.left', android: 'arrow_back', web: 'arrow_back' }}
              size={20}
              weight="semibold"
              tintColor={theme.text}
            />
          </Pressable>
          <View style={styles.titleWrap}>
            <ThemedText type="smallBold" numberOfLines={1}>
              {title}
            </ThemedText>
            {instance ? (
              <ThemedText type="small" themeColor="textSecondary" numberOfLines={1}>
                {instance.nickname}
              </ThemedText>
            ) : null}
          </View>
        </View>
        <TerminalTabBar
          tabs={tabs}
          activeId={activeTabId}
          onSelect={handleTabSelect}
          onLongPress={api ? handleTabLongPress : undefined}
          onNewTab={api ? () => setPickerVisible(true) : undefined}
        />
        {streamError ? (
          <View style={styles.banner}>
            <ThemedText type="small" themeColor="textSecondary">
              {streamError}
            </ThemedText>
          </View>
        ) : null}
      </SafeAreaView>

      <View style={[styles.terminalSlot, { backgroundColor: slotBackground }]}>
        {/*
          NOTE: TerminalView must stay mounted for the entire lifetime of
          this screen — never gate it behind a ternary, never give it a
          `key={activeTab.id}`, never swap it out when `activeTab` is
          null. The Expo DOM-component ref is a Proxy installed exactly
          once, the first time `terminalRef.current` is assigned. React
          does NOT clear our ref on unmount because the wrapper assigns
          it imperatively (not via useImperativeHandle), so any remount
          reuses the stale Proxy bound to a dead webview and every
          `write()` silently no-ops. Empty / loading state is rendered
          as an absolute overlay on top of the stable TerminalView
          instead — the xterm underneath just sits empty (the
          subscription effect bails early when sessionId is null). The
          subscription effect clears xterm content whenever sessionId
          changes, so switching tabs stays clean.
        */}
        <TerminalView
          ref={terminalRef}
          themeMode={themeMode}
          terminalThemeId={terminalThemeId}
          onInput={onInput}
          onResize={onResize}
          dom={{
            style: { flex: 1 },
            matchContents: false,
            keyboardDisplayRequiresUserAction: false,
            hideKeyboardAccessoryView: true,
            automaticallyAdjustContentInsets: false,
            scrollEnabled: false,
          }}
        />
        {activeTab ? null : (
          <View
            pointerEvents="none"
            style={[
              StyleSheet.absoluteFill,
              styles.emptyOverlay,
              { backgroundColor: slotBackground },
            ]}
          >
            <ThemedText type="small" themeColor="textSecondary">
              {sessionState.kind === 'ready'
                ? 'No tabs in this worktree'
                : 'Waiting for connection…'}
            </ThemedText>
          </View>
        )}
        {sessionState.kind === 'reconnecting' ? (
          <View
            pointerEvents="box-only"
            style={[
              StyleSheet.absoluteFill,
              styles.reconnectingOverlay,
              { backgroundColor: slotBackground },
            ]}
          >
            <ThemedText type="small" themeColor="textSecondary">
              Reconnecting…
            </ThemedText>
          </View>
        ) : null}
      </View>

      <ToolPickerSheet
        visible={pickerVisible}
        tools={tools}
        profiles={profiles}
        hydrated={hydrated}
        onSelect={handleToolPick}
        onClose={() => setPickerVisible(false)}
      />
    </ThemedView>
  )
}

function deriveWorktreeLabel(path: string): string {
  if (!path) return 'Worktree'
  const trimmed = path.replace(/\/+$/, '')
  const idx = trimmed.lastIndexOf('/')
  return idx >= 0 ? (trimmed.slice(idx + 1) ?? trimmed) : trimmed
}

const styles = StyleSheet.create({
  container: {
    flex: 1,
  },
  safeArea: {
    width: '100%',
  },
  header: {
    flexDirection: 'row',
    alignItems: 'center',
    gap: Spacing.three,
    paddingHorizontal: Spacing.four,
    paddingTop: Spacing.three,
    paddingBottom: Spacing.two,
  },
  iconBack: {
    width: Spacing.five,
    height: Spacing.five,
    alignItems: 'center',
    justifyContent: 'center',
  },
  titleWrap: {
    flex: 1,
    gap: Spacing.half,
  },
  pressed: {
    opacity: 0.6,
  },
  banner: {
    paddingHorizontal: Spacing.four,
    paddingVertical: Spacing.two,
  },
  terminalSlot: {
    flex: 1,
  },
  emptyOverlay: {
    alignItems: 'center',
    justifyContent: 'center',
    padding: Spacing.four,
  },
  reconnectingOverlay: {
    alignItems: 'center',
    justifyContent: 'center',
    padding: Spacing.four,
  },
})
