import { SymbolView } from 'expo-symbols'
import { useEffect, useRef } from 'react'
import { Pressable, ScrollView, StyleSheet, View } from 'react-native'

import { ThemedText } from '@/components/themed-text'
import { resolveToolIcon } from '@/constants/tool-icons'
import { Spacing } from '@/constants/theme'
import { useTheme } from '@/hooks/use-theme'
import type { TabSnapshot } from '@/lib/mock/snapshot-types'

type Props = {
  tabs: TabSnapshot[]
  activeId: string | null
  onSelect: (id: string) => void
  onLongPress?: (id: string) => void
  onNewTab?: () => void
}

export function TerminalTabBar({
  tabs,
  activeId,
  onSelect,
  onLongPress,
  onNewTab,
}: Props): React.ReactElement | null {
  const theme = useTheme()
  const scrollViewRef = useRef<ScrollView>(null)

  // When the active tab is the trailing entry (e.g. right after a new
  // spawn), scroll the bar to reveal it. Idempotent — scrollToEnd on an
  // already-scrolled view is a no-op.
  useEffect(() => {
    if (tabs.length === 0) return
    if (tabs[tabs.length - 1]?.id === activeId) {
      scrollViewRef.current?.scrollToEnd({ animated: true })
    }
  }, [tabs, activeId])

  if (tabs.length === 0 && !onNewTab) return null

  return (
    <ScrollView
      ref={scrollViewRef}
      horizontal
      showsHorizontalScrollIndicator={false}
      contentContainerStyle={styles.content}
    >
      {tabs.map((tab) => {
        const active = tab.id === activeId
        return (
          <Pressable
            key={tab.id}
            onPress={() => onSelect(tab.id)}
            onLongPress={onLongPress ? () => onLongPress(tab.id) : undefined}
            delayLongPress={350}
            style={({ pressed }) => [
              styles.tab,
              {
                backgroundColor: active ? theme.backgroundSelected : theme.backgroundElement,
              },
              pressed && styles.pressed,
            ]}
          >
            <SymbolView
              name={resolveToolIcon(tab.toolId)}
              size={14}
              weight={active ? 'semibold' : 'regular'}
              tintColor={active ? theme.text : theme.textSecondary}
            />
            <ThemedText
              type={active ? 'smallBold' : 'small'}
              themeColor={active ? 'text' : 'textSecondary'}
              numberOfLines={1}
            >
              {tab.name}
            </ThemedText>
          </Pressable>
        )
      })}
      {onNewTab ? (
        <Pressable
          onPress={onNewTab}
          style={({ pressed }) => [
            styles.newTab,
            { backgroundColor: theme.backgroundElement },
            pressed && styles.pressed,
          ]}
          accessibilityRole="button"
          accessibilityLabel="New tool"
        >
          <SymbolView
            name={{ ios: 'plus', android: 'add', web: 'add' }}
            size={14}
            weight="semibold"
            tintColor={theme.textSecondary}
          />
        </Pressable>
      ) : null}
      <View style={styles.trailingSpacer} />
    </ScrollView>
  )
}

const styles = StyleSheet.create({
  content: {
    paddingHorizontal: Spacing.three,
    paddingVertical: Spacing.two,
    gap: Spacing.two,
    alignItems: 'center',
  },
  tab: {
    flexDirection: 'row',
    alignItems: 'center',
    gap: Spacing.two,
    paddingHorizontal: Spacing.three,
    paddingVertical: Spacing.two,
    borderRadius: Spacing.three,
  },
  newTab: {
    alignItems: 'center',
    justifyContent: 'center',
    paddingHorizontal: Spacing.three,
    paddingVertical: Spacing.two,
    borderRadius: Spacing.three,
    minWidth: Spacing.five,
  },
  pressed: {
    opacity: 0.7,
  },
  trailingSpacer: {
    width: Spacing.two,
  },
})
