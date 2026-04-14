import { SymbolView } from 'expo-symbols'
import { Pressable, ScrollView, StyleSheet, View } from 'react-native'

import { ThemedText } from '@/components/themed-text'
import { Spacing } from '@/constants/theme'
import { useTheme } from '@/hooks/use-theme'
import type { TabSnapshot } from '@/lib/mock/snapshot-types'

type Props = {
  tabs: TabSnapshot[]
  activeId: string | null
  onSelect: (id: string) => void
}

type IconDescriptor = Parameters<typeof SymbolView>[0]['name']

function iconForTool(toolId: string): IconDescriptor {
  switch (toolId) {
    case 'claude':
      return { ios: 'sparkles', android: 'auto_awesome', web: 'auto_awesome' }
    case 'shell':
    default:
      return { ios: 'terminal', android: 'terminal', web: 'terminal' }
  }
}

export function TerminalTabBar({ tabs, activeId, onSelect }: Props): React.ReactElement | null {
  const theme = useTheme()

  if (tabs.length === 0) return null

  return (
    <ScrollView
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
            style={({ pressed }) => [
              styles.tab,
              {
                backgroundColor: active ? theme.backgroundSelected : theme.backgroundElement,
              },
              pressed && styles.pressed,
            ]}
          >
            <SymbolView
              name={iconForTool(tab.toolId)}
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
  pressed: {
    opacity: 0.7,
  },
  trailingSpacer: {
    width: Spacing.two,
  },
})
