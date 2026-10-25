import { Redirect, useLocalSearchParams } from 'expo-router'
export default function InstanceDetailScreen() {
  const { id } = useLocalSearchParams<{ id: string }>()
  return <Redirect href={{ pathname: '/remote/host', params: { hostId: id } }} />
}
