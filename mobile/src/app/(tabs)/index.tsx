import { useRouter } from 'expo-router'
import { SymbolView } from 'expo-symbols'
import ReanimatedSwipeable, {
  type SwipeableMethods,
} from 'react-native-gesture-handler/ReanimatedSwipeable'
import { Alert, FlatList, Pressable, StyleSheet, View } from 'react-native'
import { initialWindowMetrics } from 'react-native-safe-area-context'

import { AddInstanceFab } from '@/components/instances/add-instance-fab'
import { InstancesEmptyState } from '@/components/instances/empty-state'
import { InstanceCard } from '@/components/instances/instance-card'
import { ThemedText } from '@/components/themed-text'
import { ThemedView } from '@/components/themed-view'
import { BottomTabInset, Spacing } from '@/constants/theme'
import { useSavedInstances } from '@/hooks/use-saved-instances'
import { makeMockInstance } from '@/lib/mock/projects'
import { SavedInstancesStorage } from '@/lib/storage/saved-instances'
import type { SavedInstance } from '@/lib/storage/saved-instances-types'

const TOP_INSET = initialWindowMetrics?.insets.top ?? 0

function LargeTitle(): React.ReactElement {
  return (
    <View style={styles.largeTitle}>
      <ThemedText style={styles.largeTitleText}>Instances</ThemedText>
    </View>
  )
}

export default function InstancesScreen(): React.ReactElement {
  const router = useRouter()
  const { instances, loading, error } = useSavedInstances()

  const goToScan = (): void => {
    router.push('/scan')
  }
  const goToDetail = (id: string): void => {
    router.push(`/instance/${id}`)
  }

  const addMock = async (): Promise<void> => {
    try {
      await SavedInstancesStorage.add(makeMockInstance())
    } catch (e) {
      Alert.alert('Could not add mock', e instanceof Error ? e.message : String(e))
    }
  }

  const removeInstance = (id: string): void => {
    Alert.alert(
      'Remove instance',
      'This will delete the saved connection. You will need to re-scan the QR code to reconnect.',
      [
        { text: 'Cancel', style: 'cancel' },
        {
          text: 'Remove',
          style: 'destructive',
          onPress: () => void SavedInstancesStorage.remove(id).catch(() => {}),
        },
      ],
    )
  }

  const showEmpty = !loading && instances !== null && instances.length === 0

  return (
    <>
      {error && (
        <ThemedView type="backgroundElement" style={styles.errorBanner}>
          <ThemedText type="small" themeColor="textSecondary">
            Storage error: {error.message}
          </ThemedText>
        </ThemedView>
      )}

      {showEmpty ? (
        <View style={styles.emptyContainer}>
          <LargeTitle />
          <InstancesEmptyState
            onScanPress={goToScan}
            onAddMockPress={__DEV__ ? addMock : undefined}
          />
        </View>
      ) : (
        <FlatList
          data={instances ?? []}
          keyExtractor={(i) => i.id}
          ListHeaderComponent={<LargeTitle />}
          contentContainerStyle={styles.listContent}
          ItemSeparatorComponent={() => <View style={styles.separator} />}
          renderItem={({ item }) => (
            <SwipeableInstanceCard
              instance={item}
              onPress={() => goToDetail(item.id)}
              onRemove={() => removeInstance(item.id)}
            />
          )}
        />
      )}

      {!showEmpty && <AddInstanceFab onPress={goToScan} />}
    </>
  )
}

function SwipeableInstanceCard({
  instance,
  onPress,
  onRemove,
}: {
  instance: SavedInstance
  onPress: () => void
  onRemove: () => void
}): React.ReactElement {
  const renderRightActions = (
    _progress: unknown,
    _translation: unknown,
    methods: SwipeableMethods,
  ): React.ReactNode => (
    <DeleteAction
      onDelete={() => {
        methods.close()
        onRemove()
      }}
    />
  )

  return (
    <ReanimatedSwipeable
      renderRightActions={renderRightActions}
      overshootRight={false}
      friction={2}
      containerStyle={styles.swipeableContainer}
      childrenContainerStyle={styles.swipeableContainer}
    >
      <InstanceCard instance={instance} onPress={onPress} />
    </ReanimatedSwipeable>
  )
}

function DeleteAction({ onDelete }: { onDelete: () => void }): React.ReactElement {
  return (
    <Pressable
      onPress={onDelete}
      style={({ pressed }) => [styles.deleteAction, pressed && styles.deletePressed]}
      accessibilityRole="button"
      accessibilityLabel="Remove instance"
    >
      <SymbolView
        name={{ ios: 'trash', android: 'delete', web: 'delete' }}
        size={20}
        weight="semibold"
        tintColor="#fff"
      />
      <ThemedText type="small" style={styles.deleteLabel}>
        Remove
      </ThemedText>
    </Pressable>
  )
}

const styles = StyleSheet.create({
  container: {
    flex: 1,
  },
  largeTitle: {
    paddingTop: Spacing.three,
    paddingBottom: Spacing.three,
  },
  largeTitleText: {
    fontSize: 34,
    fontWeight: '600',
    letterSpacing: 0.37,
  },
  emptyContainer: {
    flex: 1,
  },
  listContent: {
    paddingTop: TOP_INSET,
    paddingHorizontal: Spacing.four,
    paddingBottom: BottomTabInset + Spacing.six + Spacing.four,
  },
  separator: {
    height: Spacing.three,
  },
  errorBanner: {
    marginHorizontal: Spacing.four,
    marginBottom: Spacing.three,
    padding: Spacing.three,
    borderRadius: Spacing.two,
  },
  swipeableContainer: {
    borderRadius: Spacing.three,
    overflow: 'hidden',
  },
  deleteAction: {
    width: 80,
    justifyContent: 'center',
    alignItems: 'center',
    gap: Spacing.one,
    backgroundColor: '#ff3b30',
    borderTopLeftRadius: Spacing.three,
    borderBottomLeftRadius: Spacing.three,
    marginLeft: Spacing.two,
  },
  deletePressed: {
    opacity: 0.8,
  },
  deleteLabel: {
    color: '#fff',
  },
})
