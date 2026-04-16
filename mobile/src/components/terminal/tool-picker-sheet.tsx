import { SymbolView } from 'expo-symbols'
import { useEffect, useState } from 'react'
import { Modal, Platform, Pressable, ScrollView, StyleSheet, View } from 'react-native'
import { SafeAreaView } from 'react-native-safe-area-context'

import { ThemedText } from '@/components/themed-text'
import { ThemedView } from '@/components/themed-view'
import { resolveToolIcon } from '@/constants/tool-icons'
import { Spacing } from '@/constants/theme'
import { useTheme } from '@/hooks/use-theme'
import type { ProfileSnapshot, ToolSnapshot } from '@/lib/remote/protocol/state-snapshot'

type Props = {
  visible: boolean
  tools: ToolSnapshot[]
  profiles: ProfileSnapshot[]
  hydrated: boolean
  onSelect: (toolId: string, profileId?: string) => void
  onClose: () => void
}

const AI_TOOL_IDS = new Set<string>(['claude', 'gemini', 'opencode', 'codex'])

export function ToolPickerSheet({
  visible,
  tools,
  profiles,
  hydrated,
  onSelect,
  onClose,
}: Props): React.ReactElement {
  const theme = useTheme()
  const [expanded, setExpanded] = useState<string | null>(null)

  // Collapse any open accordion whenever the sheet re-opens so the user
  // always starts from a clean state. Desktop persists expansion in
  // localStorage; the mobile sheet is transient so there's no value in
  // remembering it across opens.
  useEffect(() => {
    if (visible) setExpanded(null)
  }, [visible])

  const showLoading = !hydrated
  const showEmpty = hydrated && tools.length === 0

  const profilesForTool = (toolId: string): ProfileSnapshot[] =>
    AI_TOOL_IDS.has(toolId) ? profiles.filter((p) => p.agentType === toolId) : []

  return (
    <Modal
      visible={visible}
      presentationStyle="pageSheet"
      animationType="slide"
      onRequestClose={onClose}
    >
      <ThemedView style={styles.container}>
        <SafeAreaView style={styles.safeArea} edges={Platform.OS === 'ios' ? [] : ['top']}>
          <View style={styles.header}>
            <View style={styles.titleWrap}>
              <ThemedText type="smallBold">New tool</ThemedText>
            </View>
            <Pressable
              onPress={onClose}
              style={({ pressed }) => [styles.cancelBtn, pressed && styles.pressed]}
              accessibilityRole="button"
              accessibilityLabel="Cancel"
            >
              <ThemedText type="linkPrimary">Cancel</ThemedText>
            </Pressable>
          </View>

          <ScrollView contentContainerStyle={styles.scrollContent}>
            {showLoading ? (
              <View style={styles.placeholder}>
                <ThemedText type="small" themeColor="textSecondary">
                  Loading tools…
                </ThemedText>
              </View>
            ) : showEmpty ? (
              <View style={styles.placeholder}>
                <ThemedText type="small" themeColor="textSecondary">
                  No tools available on this host
                </ThemedText>
              </View>
            ) : (
              <ThemedView type="backgroundElement" style={styles.card}>
                {tools.map((tool, idx) => {
                  const enabled = tool.available && tool.category !== 'browser'
                  const toolProfiles = enabled ? profilesForTool(tool.id) : []
                  const isMulti = toolProfiles.length >= 2
                  const isExpanded = expanded === tool.id

                  const onRowPress = (): void => {
                    if (!enabled) return
                    if (isMulti) {
                      setExpanded((cur) => (cur === tool.id ? null : tool.id))
                      return
                    }
                    onSelect(tool.id, toolProfiles[0]?.id)
                  }

                  const rowBody = (
                    <View style={[styles.row, !enabled && styles.rowDisabled]}>
                      <SymbolView
                        name={resolveToolIcon(tool.id)}
                        size={20}
                        weight="regular"
                        tintColor={enabled ? theme.text : theme.textSecondary}
                      />
                      <View style={styles.rowName}>
                        <ThemedText type="smallBold" numberOfLines={1}>
                          {tool.name}
                        </ThemedText>
                      </View>
                      {isMulti ? (
                        <View
                          style={[
                            styles.chevron,
                            {
                              transform: [{ rotate: isExpanded ? '90deg' : '0deg' }],
                            },
                          ]}
                        >
                          <SymbolView
                            name={{
                              ios: 'chevron.right',
                              android: 'chevron_right',
                              web: 'chevron_right',
                            }}
                            size={14}
                            weight="semibold"
                            tintColor={theme.textSecondary}
                          />
                        </View>
                      ) : null}
                    </View>
                  )

                  return (
                    <View key={tool.id}>
                      {idx > 0 ? <View style={styles.divider} /> : null}
                      {enabled ? (
                        <Pressable
                          onPress={onRowPress}
                          style={({ pressed }) => [pressed && styles.pressed]}
                          accessibilityRole="button"
                          accessibilityLabel={
                            isMulti ? `${tool.name} profiles` : `Launch ${tool.name}`
                          }
                        >
                          {rowBody}
                        </Pressable>
                      ) : (
                        rowBody
                      )}
                      {isMulti && isExpanded ? (
                        <View style={styles.profileList}>
                          {toolProfiles.map((profile) => (
                            <Pressable
                              key={profile.id}
                              onPress={() => onSelect(tool.id, profile.id)}
                              style={({ pressed }) => [
                                styles.profileRow,
                                pressed && styles.pressed,
                              ]}
                              accessibilityRole="button"
                              accessibilityLabel={`Launch ${tool.name} with ${profile.name}`}
                            >
                              <View
                                style={[
                                  styles.profileDot,
                                  { backgroundColor: theme.textSecondary },
                                ]}
                              />
                              <ThemedText type="small" numberOfLines={1}>
                                {profile.name}
                              </ThemedText>
                            </Pressable>
                          ))}
                        </View>
                      ) : null}
                    </View>
                  )
                })}
              </ThemedView>
            )}
          </ScrollView>
        </SafeAreaView>
      </ThemedView>
    </Modal>
  )
}

const styles = StyleSheet.create({
  container: {
    flex: 1,
  },
  safeArea: {
    flex: 1,
  },
  header: {
    flexDirection: 'row',
    alignItems: 'center',
    paddingHorizontal: Spacing.four,
    paddingTop: Spacing.three,
    paddingBottom: Spacing.three,
  },
  titleWrap: {
    flex: 1,
  },
  cancelBtn: {
    paddingVertical: Spacing.one,
    paddingHorizontal: Spacing.two,
  },
  scrollContent: {
    paddingHorizontal: Spacing.four,
    paddingBottom: Spacing.four,
  },
  card: {
    borderRadius: Spacing.three,
    overflow: 'hidden',
  },
  row: {
    flexDirection: 'row',
    alignItems: 'center',
    gap: Spacing.three,
    paddingVertical: Spacing.three,
    paddingHorizontal: Spacing.three,
  },
  rowDisabled: {
    opacity: 0.5,
  },
  rowName: {
    flex: 1,
  },
  chevron: {
    width: 14,
    height: 14,
    alignItems: 'center',
    justifyContent: 'center',
  },
  divider: {
    height: StyleSheet.hairlineWidth,
    backgroundColor: 'rgba(127, 127, 127, 0.2)',
    marginLeft: Spacing.three,
  },
  profileList: {
    paddingBottom: Spacing.two,
  },
  profileRow: {
    flexDirection: 'row',
    alignItems: 'center',
    gap: Spacing.three,
    paddingVertical: Spacing.two,
    paddingLeft: Spacing.five + Spacing.three,
    paddingRight: Spacing.three,
  },
  profileDot: {
    width: 4,
    height: 4,
    borderRadius: 2,
  },
  placeholder: {
    paddingVertical: Spacing.five,
    alignItems: 'center',
  },
  pressed: {
    opacity: 0.6,
  },
})
