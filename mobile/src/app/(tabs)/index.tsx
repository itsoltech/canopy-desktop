import { useRouter } from 'expo-router'
import { Alert, FlatList, StyleSheet, View } from 'react-native'
import { SafeAreaView } from 'react-native-safe-area-context'

import { AddInstanceFab } from '@/components/instances/add-instance-fab'
import { InstancesEmptyState } from '@/components/instances/empty-state'
import { InstanceCard } from '@/components/instances/instance-card'
import { ThemedText } from '@/components/themed-text'
import { ThemedView } from '@/components/themed-view'
import { BottomTabInset, Spacing } from '@/constants/theme'
import { useSavedInstances } from '@/hooks/use-saved-instances'
import { makeMockInstance } from '@/lib/mock/projects'
import { SavedInstancesStorage } from '@/lib/storage/saved-instances'

export default function InstancesScreen() {
  const router = useRouter()
  const { instances, loading, error } = useSavedInstances()

  const goToScan = () => router.push('/scan')
  const goToDetail = (id: string) => router.push(`/instance/${id}`)

  const addMock = async () => {
    try {
      await SavedInstancesStorage.add(makeMockInstance())
    } catch (e) {
      Alert.alert('Could not add mock', e instanceof Error ? e.message : String(e))
    }
  }

  const showEmpty = !loading && instances !== null && instances.length === 0

  return (
    <ThemedView style={styles.container}>
      <SafeAreaView style={styles.safeArea} edges={['top']}>
        <View style={styles.header}>
          <ThemedText type="subtitle">Instances</ThemedText>
        </View>

        {error && (
          <ThemedView type="backgroundElement" style={styles.errorBanner}>
            <ThemedText type="small" themeColor="textSecondary">
              Storage error: {error.message}
            </ThemedText>
          </ThemedView>
        )}

        {showEmpty ? (
          <InstancesEmptyState
            onScanPress={goToScan}
            onAddMockPress={__DEV__ ? addMock : undefined}
          />
        ) : (
          <FlatList
            data={instances ?? []}
            keyExtractor={(i) => i.id}
            contentContainerStyle={styles.listContent}
            ItemSeparatorComponent={() => <View style={styles.separator} />}
            renderItem={({ item }) => (
              <InstanceCard instance={item} onPress={() => goToDetail(item.id)} />
            )}
          />
        )}
      </SafeAreaView>

      {!showEmpty && <AddInstanceFab onPress={goToScan} />}
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
  header: {
    paddingHorizontal: Spacing.four,
    paddingVertical: Spacing.three,
  },
  listContent: {
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
})
