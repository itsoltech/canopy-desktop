import { useLocalSearchParams, useRouter } from 'expo-router'
import { SymbolView } from 'expo-symbols'
import { useEffect, useMemo, useState } from 'react'
import {
  ActivityIndicator,
  FlatList,
  Modal,
  Pressable,
  ScrollView,
  StyleSheet,
  TextInput,
  View,
} from 'react-native'
import { SafeAreaView } from 'react-native-safe-area-context'

import { ThemedText } from '@/components/themed-text'
import { ThemedView } from '@/components/themed-view'
import { Spacing } from '@/constants/theme'
import { useRemoteSession } from '@/hooks/use-remote-session'
import { useProjects } from '@/hooks/use-remote-state'
import { useTheme } from '@/hooks/use-theme'
import { safeDirName } from '@/lib/path-utils'

type Mode = 'new' | 'existing'

type BranchList = {
  local: string[]
  remote: string[]
  current: string | null
}

const BASE_DIR = '~/canopy/worktrees'

export default function NewWorktreeScreen(): React.ReactElement {
  const router = useRouter()
  const theme = useTheme()
  const { projectId } = useLocalSearchParams<{ projectId: string }>()
  const projects = useProjects()
  const { api } = useRemoteSession()

  const project = useMemo(() => projects.find((p) => p.id === projectId), [projects, projectId])

  const [mode, setMode] = useState<Mode>('new')
  const [branches, setBranches] = useState<BranchList | null>(null)
  const [loadError, setLoadError] = useState<string | null>(null)
  const [selectedBase, setSelectedBase] = useState('')
  const [newBranchName, setNewBranchName] = useState('')
  const [customPath, setCustomPath] = useState('')
  const [submitting, setSubmitting] = useState(false)
  const [submitError, setSubmitError] = useState<string | null>(null)
  const [pickerOpen, setPickerOpen] = useState(false)
  const [branchQuery, setBranchQuery] = useState('')

  const repoRoot = project?.repoRoot ?? null

  useEffect(() => {
    if (!api || !repoRoot) return
    let cancelled = false
    void (async () => {
      try {
        const list = await api.git.listBranches(repoRoot)
        if (cancelled) return
        setBranches(list)
        setSelectedBase((prev) => prev || list.current || list.local[0] || '')
      } catch (e) {
        if (cancelled) return
        setLoadError(e instanceof Error ? e.message : String(e))
      }
    })()
    return () => {
      cancelled = true
    }
  }, [api, repoRoot])

  const effectiveBranchName = useMemo(() => {
    if (mode === 'new') return newBranchName
    if (!selectedBase) return ''
    const slash = selectedBase.indexOf('/')
    const isRemoteOnly = slash !== -1 && branches ? !branches.local.includes(selectedBase) : false
    return isRemoteOnly ? selectedBase.slice(slash + 1) : selectedBase
  }, [mode, newBranchName, selectedBase, branches])

  const defaultPath = useMemo(() => {
    if (!project || !effectiveBranchName) return ''
    return `${BASE_DIR}/${project.name}/${safeDirName(effectiveBranchName)}`
  }, [project, effectiveBranchName])

  const worktreePath = customPath.trim().length > 0 ? customPath.trim() : defaultPath

  const canSubmit =
    !!project &&
    !!repoRoot &&
    !!api &&
    !submitting &&
    effectiveBranchName.length > 0 &&
    worktreePath.length > 0 &&
    (mode === 'new' ? !!selectedBase : true)

  const submit = async (): Promise<void> => {
    if (!canSubmit || !api || !repoRoot) return
    setSubmitting(true)
    setSubmitError(null)
    try {
      if (mode === 'new') {
        await api.worktree.add({
          repoRoot,
          path: worktreePath,
          branch: newBranchName,
          baseBranch: selectedBase,
        })
      } else {
        const isRemoteOnly =
          branches && selectedBase.includes('/') && !branches.local.includes(selectedBase)
        await api.worktree.addCheckout({
          repoRoot,
          path: worktreePath,
          branch: selectedBase,
          createLocalTracking: Boolean(isRemoteOnly),
        })
      }
      router.back()
    } catch (e) {
      setSubmitError(e instanceof Error ? e.message : String(e))
      setSubmitting(false)
    }
  }

  if (!project) {
    return (
      <ThemedView style={styles.centered}>
        <ThemedText type="subtitle">Project not found</ThemedText>
        <Pressable
          onPress={() => router.back()}
          style={({ pressed }) => [styles.linkButton, pressed && styles.pressed]}
        >
          <ThemedText type="linkPrimary">Go back</ThemedText>
        </Pressable>
      </ThemedView>
    )
  }

  if (!api) {
    return (
      <ThemedView style={styles.centered}>
        <ThemedText type="subtitle">Not connected</ThemedText>
        <ThemedText type="small" themeColor="textSecondary" style={styles.hint}>
          Open the instance first to establish a session.
        </ThemedText>
        <Pressable
          onPress={() => router.back()}
          style={({ pressed }) => [styles.linkButton, pressed && styles.pressed]}
        >
          <ThemedText type="linkPrimary">Go back</ThemedText>
        </Pressable>
      </ThemedView>
    )
  }

  const filteredBranches = buildFilteredList(branches, branchQuery)

  return (
    <ThemedView style={styles.container}>
      <SafeAreaView style={styles.safeArea} edges={['top']}>
        <View style={styles.header}>
          <Pressable
            onPress={() => router.back()}
            style={({ pressed }) => [styles.iconBack, pressed && styles.pressed]}
            accessibilityLabel="Cancel"
          >
            <SymbolView
              name={{ ios: 'chevron.left', android: 'arrow_back', web: 'arrow_back' }}
              size={20}
              weight="semibold"
              tintColor={theme.text}
            />
          </Pressable>
          <View style={styles.headerTitle}>
            <ThemedText type="subtitle">New worktree</ThemedText>
            <ThemedText type="small" themeColor="textSecondary" numberOfLines={1}>
              {project.name}
            </ThemedText>
          </View>
        </View>

        <ScrollView
          contentContainerStyle={styles.scrollContent}
          keyboardShouldPersistTaps="handled"
        >
          <SegmentedControl
            value={mode}
            onChange={setMode}
            options={[
              { value: 'new', label: 'New branch' },
              { value: 'existing', label: 'Existing branch' },
            ]}
          />

          {branches === null && loadError === null && (
            <View style={styles.loadingRow}>
              <ActivityIndicator />
              <ThemedText type="small" themeColor="textSecondary">
                Loading branches…
              </ThemedText>
            </View>
          )}
          {loadError && (
            <ThemedView type="backgroundElement" style={styles.errorBox}>
              <ThemedText type="small" style={styles.errorText}>
                {loadError}
              </ThemedText>
            </ThemedView>
          )}

          {branches && (
            <>
              <FieldLabel>{mode === 'new' ? 'Base branch' : 'Branch to check out'}</FieldLabel>
              <Pressable
                onPress={() => setPickerOpen(true)}
                style={({ pressed }) => [pressed && styles.pressed]}
              >
                <ThemedView type="backgroundElement" style={styles.pickerField}>
                  <ThemedText type="small" numberOfLines={1} style={styles.pickerText}>
                    {selectedBase || 'Select a branch'}
                  </ThemedText>
                  <SymbolView
                    name={{ ios: 'chevron.down', android: 'expand_more', web: 'expand_more' }}
                    size={14}
                    weight="medium"
                    tintColor={theme.textSecondary}
                  />
                </ThemedView>
              </Pressable>

              {mode === 'new' && (
                <>
                  <FieldLabel>New branch name</FieldLabel>
                  <ThemedView type="backgroundElement" style={styles.inputWrap}>
                    <TextInput
                      value={newBranchName}
                      onChangeText={setNewBranchName}
                      placeholder="feature/my-branch"
                      placeholderTextColor={theme.textSecondary}
                      autoCapitalize="none"
                      autoCorrect={false}
                      style={[styles.input, { color: theme.text }]}
                    />
                  </ThemedView>
                </>
              )}

              <FieldLabel>Path</FieldLabel>
              <ThemedView type="backgroundElement" style={styles.inputWrap}>
                <TextInput
                  value={customPath}
                  onChangeText={setCustomPath}
                  placeholder={defaultPath || 'worktree path'}
                  placeholderTextColor={theme.textSecondary}
                  autoCapitalize="none"
                  autoCorrect={false}
                  style={[styles.input, styles.inputMono, { color: theme.text }]}
                />
              </ThemedView>
              <ThemedText type="small" themeColor="textSecondary" style={styles.hintSmall}>
                Leave empty to use the default location.
              </ThemedText>
            </>
          )}

          {submitError && (
            <ThemedView type="backgroundElement" style={styles.errorBox}>
              <ThemedText type="small" style={styles.errorText}>
                {submitError}
              </ThemedText>
            </ThemedView>
          )}

          <View style={styles.actions}>
            <Pressable
              onPress={() => router.back()}
              style={({ pressed }) => [styles.actionBtn, pressed && styles.pressed]}
            >
              <ThemedView type="backgroundElement" style={styles.actionInner}>
                <ThemedText type="smallBold">Cancel</ThemedText>
              </ThemedView>
            </Pressable>
            <Pressable
              onPress={submit}
              disabled={!canSubmit}
              style={({ pressed }) => [styles.actionBtn, pressed && styles.pressed]}
            >
              <ThemedView
                type="backgroundSelected"
                style={[styles.actionInner, !canSubmit && styles.disabled]}
              >
                {submitting ? (
                  <ActivityIndicator />
                ) : (
                  <ThemedText type="smallBold">Create</ThemedText>
                )}
              </ThemedView>
            </Pressable>
          </View>
        </ScrollView>
      </SafeAreaView>

      <Modal visible={pickerOpen} animationType="slide" onRequestClose={() => setPickerOpen(false)}>
        <ThemedView style={styles.modalContainer}>
          <SafeAreaView style={styles.safeArea} edges={['top']}>
            <View style={styles.header}>
              <Pressable
                onPress={() => setPickerOpen(false)}
                style={({ pressed }) => [styles.iconBack, pressed && styles.pressed]}
              >
                <SymbolView
                  name={{ ios: 'chevron.left', android: 'arrow_back', web: 'arrow_back' }}
                  size={20}
                  weight="semibold"
                  tintColor={theme.text}
                />
              </Pressable>
              <View style={styles.headerTitle}>
                <ThemedText type="subtitle">Pick branch</ThemedText>
              </View>
            </View>
            <ThemedView type="backgroundElement" style={styles.searchWrap}>
              <TextInput
                value={branchQuery}
                onChangeText={setBranchQuery}
                placeholder="Search branches"
                placeholderTextColor={theme.textSecondary}
                autoCapitalize="none"
                autoCorrect={false}
                style={[styles.input, { color: theme.text }]}
              />
            </ThemedView>
            <FlatList
              data={filteredBranches}
              keyExtractor={(item) => item.key}
              renderItem={({ item }) => (
                <Pressable
                  onPress={() => {
                    setSelectedBase(item.name)
                    setPickerOpen(false)
                    setBranchQuery('')
                  }}
                  style={({ pressed }) => [styles.branchRow, pressed && styles.pressed]}
                >
                  <View style={styles.branchText}>
                    <ThemedText type="small" numberOfLines={1}>
                      {item.name}
                    </ThemedText>
                    {item.isCurrent && (
                      <ThemedText type="small" themeColor="textSecondary">
                        current
                      </ThemedText>
                    )}
                    {item.isRemote && (
                      <ThemedText type="small" themeColor="textSecondary">
                        remote
                      </ThemedText>
                    )}
                  </View>
                  {item.name === selectedBase && (
                    <SymbolView
                      name={{ ios: 'checkmark', android: 'check', web: 'check' }}
                      size={14}
                      weight="semibold"
                      tintColor={theme.text}
                    />
                  )}
                </Pressable>
              )}
            />
          </SafeAreaView>
        </ThemedView>
      </Modal>
    </ThemedView>
  )
}

type BranchItem = {
  key: string
  name: string
  isRemote: boolean
  isCurrent: boolean
}

function buildFilteredList(branches: BranchList | null, query: string): BranchItem[] {
  if (!branches) return []
  const q = query.trim().toLowerCase()
  const items: BranchItem[] = [
    ...branches.local.map((name) => ({
      key: `local:${name}`,
      name,
      isRemote: false,
      isCurrent: name === branches.current,
    })),
    ...branches.remote.map((name) => ({
      key: `remote:${name}`,
      name,
      isRemote: true,
      isCurrent: false,
    })),
  ]
  if (!q) return items
  return items.filter((i) => i.name.toLowerCase().includes(q))
}

function FieldLabel({ children }: { children: React.ReactNode }): React.ReactElement {
  return (
    <ThemedText type="smallBold" themeColor="textSecondary" style={styles.fieldLabel}>
      {children}
    </ThemedText>
  )
}

type SegmentedControlProps<T extends string> = {
  value: T
  onChange: (next: T) => void
  options: Array<{ value: T; label: string }>
}

function SegmentedControl<T extends string>({
  value,
  onChange,
  options,
}: SegmentedControlProps<T>): React.ReactElement {
  return (
    <ThemedView type="backgroundElement" style={styles.segmented}>
      {options.map((opt) => {
        const selected = opt.value === value
        return (
          <Pressable
            key={opt.value}
            onPress={() => onChange(opt.value)}
            style={({ pressed }) => [styles.segmentBtn, pressed && styles.pressed]}
          >
            <ThemedView
              type={selected ? 'backgroundSelected' : 'backgroundElement'}
              style={styles.segmentInner}
            >
              <ThemedText type={selected ? 'smallBold' : 'small'}>{opt.label}</ThemedText>
            </ThemedView>
          </Pressable>
        )
      })}
    </ThemedView>
  )
}

const styles = StyleSheet.create({
  container: {
    flex: 1,
  },
  safeArea: {
    flex: 1,
  },
  centered: {
    flex: 1,
    alignItems: 'center',
    justifyContent: 'center',
    gap: Spacing.two,
    padding: Spacing.four,
  },
  hint: {
    textAlign: 'center',
  },
  linkButton: {
    marginTop: Spacing.three,
    padding: Spacing.two,
  },
  header: {
    flexDirection: 'row',
    paddingHorizontal: Spacing.four,
    paddingTop: Spacing.three,
    gap: Spacing.three,
  },
  iconBack: {
    width: Spacing.five,
    height: Spacing.five,
    alignItems: 'center',
    justifyContent: 'center',
    marginTop: Spacing.one,
  },
  headerTitle: {
    flex: 1,
    gap: Spacing.half,
  },
  scrollContent: {
    padding: Spacing.four,
    gap: Spacing.two,
    paddingBottom: Spacing.six,
  },
  segmented: {
    flexDirection: 'row',
    borderRadius: Spacing.three,
    overflow: 'hidden',
    padding: Spacing.half,
    gap: Spacing.half,
  },
  segmentBtn: {
    flex: 1,
  },
  segmentInner: {
    borderRadius: Spacing.two,
    paddingVertical: Spacing.two,
    alignItems: 'center',
  },
  loadingRow: {
    flexDirection: 'row',
    alignItems: 'center',
    gap: Spacing.two,
    padding: Spacing.three,
  },
  fieldLabel: {
    marginTop: Spacing.three,
    fontSize: 11,
    letterSpacing: 0.5,
  },
  pickerField: {
    flexDirection: 'row',
    alignItems: 'center',
    justifyContent: 'space-between',
    gap: Spacing.two,
    paddingHorizontal: Spacing.three,
    paddingVertical: Spacing.three,
    borderRadius: Spacing.three,
  },
  pickerText: {
    flex: 1,
  },
  inputWrap: {
    borderRadius: Spacing.three,
    paddingHorizontal: Spacing.three,
    paddingVertical: Spacing.two,
  },
  input: {
    fontSize: 14,
    paddingVertical: Spacing.one,
  },
  inputMono: {
    fontFamily: 'Menlo',
    fontSize: 12,
  },
  hintSmall: {
    paddingHorizontal: Spacing.one,
  },
  errorBox: {
    borderRadius: Spacing.three,
    padding: Spacing.three,
    borderWidth: 1,
    borderColor: '#ff453a',
  },
  errorText: {
    color: '#ff453a',
  },
  actions: {
    flexDirection: 'row',
    gap: Spacing.three,
    marginTop: Spacing.four,
  },
  actionBtn: {
    flex: 1,
  },
  actionInner: {
    paddingVertical: Spacing.three,
    borderRadius: Spacing.three,
    alignItems: 'center',
  },
  disabled: {
    opacity: 0.5,
  },
  pressed: {
    opacity: 0.6,
  },
  modalContainer: {
    flex: 1,
  },
  searchWrap: {
    marginHorizontal: Spacing.four,
    marginTop: Spacing.three,
    marginBottom: Spacing.two,
    borderRadius: Spacing.three,
    paddingHorizontal: Spacing.three,
    paddingVertical: Spacing.two,
  },
  branchRow: {
    flexDirection: 'row',
    alignItems: 'center',
    justifyContent: 'space-between',
    paddingHorizontal: Spacing.four,
    paddingVertical: Spacing.three,
    gap: Spacing.two,
  },
  branchText: {
    flex: 1,
    flexDirection: 'row',
    alignItems: 'center',
    gap: Spacing.two,
  },
})
