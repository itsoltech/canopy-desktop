import { Redirect, useLocalSearchParams } from 'expo-router'
export default function NewProjectScreen() {
  const { hostId } = useLocalSearchParams<{ hostId?: string }>()
  return <Redirect href={hostId ? { pathname: '/remote/host', params: { hostId } } : '/'} />
}
