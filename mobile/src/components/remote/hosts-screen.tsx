import React, { useEffect, useState } from 'react'
import { useRouter } from 'expo-router'
import { ThemedText } from '@/components/themed-text'
import { ThemedView } from '@/components/themed-view'
import { remoteHosts, type HostSession } from '@/lib/remote/grpc/hosts'
import { Action, Page, useRevision } from './common'
export default function HostsScreen() {
  const [hosts, setHosts] = useState<HostSession[]>([]), [error, setError] = useState('')
  const router = useRouter()
  useEffect(() => {
    try {
      const owner = remoteHosts(), refresh = () => setHosts(owner.list())
      const unsubscribe = owner.subscribe(refresh)
      void owner.restore().then(refresh).catch(e => setError(String(e))); refresh()
      return unsubscribe
    } catch (e) { setError(String(e)) }
  }, [])
  return <Page title="Instances">{error && <ThemedText>{error}</ThemedText>}{hosts.map(host => <HostRow key={host.snapshot().hostId} host={host} />)}{!hosts.length && !error && <ThemedText themeColor="textSecondary">Scan the pairing code on your Canopy desktop to add a host.</ThemedText>}<Action title="Scan pairing code" run={() => router.push('/scan')} /></Page>
}
function HostRow({ host }: { host: HostSession }) {
  useRevision(host); const state = host.snapshot(), router = useRouter()
  return <ThemedView type="backgroundElement" style={{ padding: 16, borderRadius: 16 }}><Action title={host.store.snapshot().connection.name} run={() => router.push({ pathname: '/remote/host', params: { hostId: state.hostId } })} /><ThemedText type="small" themeColor="textSecondary">{state.phase}{state.stale ? ' · cached' : ''} · {state.catalog?.chats.length ?? 0} chats</ThemedText>{state.error && <ThemedText type="small">{state.error}</ThemedText>}</ThemedView>
}
