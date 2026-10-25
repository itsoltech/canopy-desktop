import { Redirect, useLocalSearchParams } from 'expo-router'
export default function NewWorktreeScreen() {
  const { hostId, workspaceId } = useLocalSearchParams<{ hostId?: string; workspaceId?: string }>()
  return <Redirect href={hostId && workspaceId ? { pathname: '/remote/workspace', params: { hostId, workspaceId } } : '/'} />
}
