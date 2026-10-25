import React, { useEffect, useRef, useState } from 'react'
import { Pressable, TextInput, ScrollView, StyleSheet, Alert, type TextInputProps } from 'react-native'
import { SafeAreaView } from 'react-native-safe-area-context'
import { useRouter } from 'expo-router'
import { ThemedText } from '@/components/themed-text'
import { ThemedView } from '@/components/themed-view'
import { Spacing } from '@/constants/theme'
import { useTheme } from '@/hooks/use-theme'
import type { HostStore } from '@/lib/remote/grpc/persistence'
import { remoteHosts, type HostSession } from '@/lib/remote/grpc/hosts'
export function Action({ title, run, disabled = false }: { title: string; run: () => void | Promise<unknown>; disabled?: boolean }) {
  const [busy, setBusy] = useState(false)
  return <Pressable accessibilityRole="button" disabled={disabled || busy} style={({ pressed }) => ({ minHeight: 48, justifyContent: 'center', opacity: disabled || busy || pressed ? 0.5 : 1 })} onPress={() => { setBusy(true); Promise.resolve().then(run).catch(e => Alert.alert('Could not complete', String(e instanceof Error ? e.message : e))).finally(() => setBusy(false)) }}><ThemedText type="linkPrimary">{busy ? 'Working…' : title}</ThemedText></Pressable>
}
export function Field(props: TextInputProps & { label: string }) {
  const theme = useTheme()
  return <TextInput {...props} accessibilityLabel={props.label} placeholder={props.placeholder ?? props.label} placeholderTextColor={theme.textSecondary} style={[{ color: theme.text, backgroundColor: theme.backgroundElement, padding: Spacing.three, borderRadius: Spacing.two, minHeight: 48, fontSize: 16 }, props.style]} />
}
export function Page({ title, children, store, viewKey, scrollRef }: React.PropsWithChildren<{ title: string; store?: HostStore; viewKey?: string; scrollRef?: React.RefObject<ScrollView | null> }>) {
  const router = useRouter(), timer = useRef<ReturnType<typeof setTimeout> | undefined>(undefined)
  const initial = useRef(store && viewKey ? store.snapshot().views?.[viewKey]?.scroll ?? 0 : 0)
  const scroll = useRef(initial.current)
  const persist = () => { if (store && viewKey) void store.update(state => { const views = state.views ??= {}; views[viewKey] = { ...views[viewKey], scroll: scroll.current }; return null }).catch(e => Alert.alert('View state not saved', String(e))) }
  useEffect(() => () => { clearTimeout(timer.current); persist() }, [store, viewKey])
  return <ThemedView style={{ flex: 1 }}><SafeAreaView style={{ flex: 1 }}><ScrollView ref={scrollRef} keyboardShouldPersistTaps="handled" contentOffset={{ x: 0, y: initial.current }} scrollEventThrottle={150} onScroll={event => { scroll.current = event.nativeEvent.contentOffset.y; clearTimeout(timer.current); timer.current = setTimeout(persist, 400) }} contentContainerStyle={styles.page}>{router.canGoBack() && <Action title="Back" run={() => router.back()} />}<ThemedText type="subtitle">{title}</ThemedText>{children}</ScrollView></SafeAreaView></ThemedView>
}
export function useHost(hostId: string): { host?: HostSession; error?: string } {
  const [value, setValue] = useState<{ host?: HostSession; error?: string }>({})
  useEffect(() => {
    let active = true, unsubscribe: (() => void) | undefined
    try {
      const hosts = remoteHosts()
      const refresh = () => { if (active) setValue({ host: hosts.get(hostId) }) }
      unsubscribe = hosts.subscribe(refresh)
      void hosts.restore().then(() => { if (active && !hosts.get(hostId)) setValue({ error: 'Host is not paired in this native client. Pair it from Instances.' }); else refresh() }).catch(e => { if (active) setValue({ error: String(e) }) }); refresh()
    } catch (e) { setValue({ error: String(e) }) }
    return () => { active = false; unsubscribe?.() }
  }, [hostId])
  return value
}
export function useRevision(owner?: { subscribe(callback: () => void): () => void }): number {
  const [revision, setRevision] = useState(0)
  useEffect(() => owner?.subscribe(() => setRevision(v => v + 1)), [owner])
  return revision
}
const styles = StyleSheet.create({ page: { padding: Spacing.four, gap: Spacing.two, paddingBottom: Spacing.six } })
