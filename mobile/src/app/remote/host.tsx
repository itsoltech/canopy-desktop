import React, { useEffect, useState } from 'react'
import { Alert, Share } from 'react-native'
import { diagnostics } from '@/lib/remote/grpc/metrics'
import { useLocalSearchParams, useRouter } from 'expo-router'
import { Page, Action, Field, useHost, useRevision } from '@/components/remote/common'
import { ThemedText } from '@/components/themed-text'
import { command, checkReceipt, uniqueId } from '@/lib/remote/grpc/commands'
import { saveSshCredential } from '@/lib/remote/grpc/credentials'
import { remoteHosts } from '@/lib/remote/grpc/hosts'
import type { AgentDiscovery, ProjectRef } from '@/lib/remote/grpc/schema'
export default function HostScreen() {
  const { hostId } = useLocalSearchParams<{ hostId: string }>(), { host, error } = useHost(hostId), router = useRouter()
  useRevision(host); useRevision(host?.store)
  const [path, setPath] = useState(''), [settings, setSettings] = useState<AgentDiscovery>(), [workspace, setWorkspace] = useState<ProjectRef>()
  const [routeKind, setRouteKind] = useState<'direct' | 'ssh'>('direct'), [address, setAddress] = useState(''), [port, setPort] = useState('')
  const [sshAddress, setSshAddress] = useState(''), [sshPort, setSshPort] = useState('22'), [username, setUsername] = useState(''), [pin, setPin] = useState(''), [password, setPassword] = useState(''), [privateKey, setPrivateKey] = useState(''), [passphrase, setPassphrase] = useState('')
  const [discoveryError, setDiscoveryError] = useState('')
  const state = host?.snapshot()
  const selectedCheckout = state?.catalog?.projects.flatMap(project => project.checkouts).find(checkout => !!workspace && checkout.project?.project_id === workspace.project_id && checkout.project?.worktree_id === workspace.worktree_id)
  const workspaceUnavailable = !!workspace && (!selectedCheckout || selectedCheckout.closing === true)
  useEffect(() => { if (host && state?.phase === 'ready') { const c = new AbortController(); void (async () => {
      const agents: AgentDiscovery['agents'] = [], seen = new Set<string>(); let page_token: string | undefined
      do {
        const page = await host.transport.unary('CatalogService/ListAgents', { environment: { host_id: hostId }, project: workspace, page_token }, c.signal)
        agents.push(...page.agents); page_token = page.next_page_token
        if (page_token && (seen.has(page_token) || seen.size >= 256)) throw Error('Agent catalog pagination did not advance')
        if (page_token) seen.add(page_token)
      } while (page_token)
      if (!c.signal.aborted) { setSettings({ agents }); setDiscoveryError('') }
    })().catch(e => { if (!c.signal.aborted) setDiscoveryError(String(e)) }); return () => c.abort() } }, [host, hostId, state?.phase, workspace?.project_id, workspace?.worktree_id])
  return <Page title={host?.store.snapshot().connection.name ?? 'Host'}>
    <ThemedText>{error ?? state?.error ?? state?.phase ?? 'Loading…'}</ThemedText>
    <Action title="Share local diagnostics" run={() => Share.share({ message: JSON.stringify(diagnostics(), null, 2) })} />
    <ThemedText type="small">Service health: {state?.health === 1 ? 'Serving' : state?.health === 2 ? 'Not serving' : 'Unknown'} · Domain: {state?.status?.reason || state?.status?.readiness || 'Unknown'}</ThemedText>
    {host && <><Action title="Reconnect" run={() => host.connect()} />{host.store.snapshot().connection.routes.map((route, index) => <Action key={index} title={`${index === host.store.snapshot().connection.route ? '✓ ' : ''}${route.kind} · ${route.address}:${route.port}`} run={() => host.selectRoute(index)} />)}
      <ThemedText selectable>Trusted host: {host.store.snapshot().connection.fingerprint}</ThemedText>
      <ThemedText>Add connection route</ThemedText>{(['direct', 'ssh'] as const).map(kind => <Action key={kind} title={`${routeKind === kind ? '✓ ' : ''}${kind === 'direct' ? 'LAN / VPN' : 'SSH tunnel'}`} run={() => setRouteKind(kind)} />)}
      <Field label="Canopy address (from SSH host for tunnels)" value={address} onChangeText={setAddress} /><Field label="Canopy port" value={port} onChangeText={setPort} keyboardType="number-pad" />
      {routeKind === 'ssh' && <><Field label="SSH address" value={sshAddress} onChangeText={setSshAddress} /><Field label="SSH port" value={sshPort} onChangeText={setSshPort} keyboardType="number-pad" /><Field label="SSH username" value={username} onChangeText={setUsername} /><Field label="Verified SSH SHA256 fingerprint" value={pin} onChangeText={setPin} /><Field label="SSH password" secureTextEntry value={password} onChangeText={setPassword} /><Field label="SSH private key (alternative to password)" secureTextEntry value={privateKey} onChangeText={setPrivateKey} /><Field label="Private key passphrase" secureTextEntry value={passphrase} onChangeText={setPassphrase} /></>}
      <Action title="Save route" run={async () => {
        const p = Number(port), sp = Number(sshPort)
        if (!address || !Number.isInteger(p) || p < 1 || p > 65535) throw Error('Invalid Canopy address or port')
        const credentialRef = uniqueId()
        if (routeKind === 'ssh') {
          if (!sshAddress || !username || !/^SHA256:[A-Za-z0-9+/]{43}$/.test(pin) || !Number.isInteger(sp) || sp < 1 || sp > 65535 || (!privateKey && !password)) throw Error('SSH address, credentials and verified host key are required')
          await saveSshCredential(credentialRef, privateKey ? { sshPrivateKey: privateKey, sshPassphrase: passphrase } : { sshPassword: password })
        }
        await host.store.update(state => { if (state.connection.routes.length >= 8) throw Error('Route limit reached'); state.connection.routes.push(routeKind === 'direct' ? { kind: 'direct', address, port: p } : { kind: 'ssh', address, port: p, sshAddress, sshPort: sp, username, fingerprint: pin, credentialRef }); return null })
        await remoteHosts().registry.save(host.store.snapshot().connection)
        setPassword(''); setPrivateKey(''); setPassphrase('')
      }} />
      <Action title="Host settings and devices" run={() => router.push({ pathname: '/remote/settings', params: { hostId } })} />
      {state?.catalog?.projects.map(project => <React.Fragment key={project.path}><ThemedText>{project.name}</ThemedText><Action title={project.checkouts.some(checkout => checkout.closing) ? "Retry cleanup…" : "Close project on host…"} disabled={state?.phase !== 'ready'} run={async () => {
        const target = project.checkouts[0]?.project; if (!target) throw Error('Project has no available checkout')
        const plan = await host.transport.unary('CatalogService/PreflightCloseProject', { project: target })
        if (!plan.can_execute) throw Error(plan.reason)
        await new Promise<void>((resolve, reject) => Alert.alert(`Close ${project.name}?`, ['All project runtimes on the host will close.', ...plan.risks.map(risk => `${risk.title}: ${risk.working ? 'working' : 'idle'}, ${risk.queued} queued, ${risk.observers} viewers`), `${plan.dirty.length} unsaved buffers`, ...plan.dirty.map(dirty => `${dirty.file?.relative_path ?? 'File'} · ${dirty.device_id} · ${dirty.online ? 'online' : 'offline'} · last seen ${dirty.last_seen_unix}`)].join('\n'), [{ text: 'Cancel', onPress: () => resolve() }, { text: 'Close project', style: 'destructive', onPress: () => { void host.mutate('CatalogService/CloseProject', { command: command(host.store), project: target, plan_id: plan.plan_id }).then(checkReceipt).then(() => resolve()).catch(reject) } }], { cancelable: false }))
      }} />{project.checkouts.map(checkout => <React.Fragment key={checkout.catalog_id}><Action title={`${selectedCheckout?.catalog_id === checkout.catalog_id ? '✓ ' : ''}${checkout.name}${checkout.closing ? ' · Closing' : ''}`} disabled={checkout.closing} run={() => setWorkspace(checkout.project)} /><Action title="Files, Git and Tasks" disabled={checkout.closing} run={() => router.push({ pathname: '/remote/workspace', params: { hostId, workspaceId: checkout.catalog_id } })} /></React.Fragment>)}</React.Fragment>)}
      <Action title={`${workspace ? '' : '✓ '}Private chat`} run={() => setWorkspace(undefined)} />
      {workspaceUnavailable && <ThemedText>Selected workspace is closing or unavailable. Choose an available workspace or a private chat.</ThemedText>}
      {!!discoveryError && <ThemedText>{discoveryError}</ThemedText>}
      {settings?.agents.map(agent => <React.Fragment key={agent.agent_id}><ThemedText>{agent.name}</ThemedText>{agent.accounts.map(account => <Action key={account.account_id} title={`New chat · ${account.label}${account.native_default ? ' (native account)' : ''}`} disabled={state?.phase !== 'ready' || workspaceUnavailable} run={async () => { if (workspaceUnavailable) throw Error('Workspace is closing or unavailable'); const receipt = checkReceipt(await host.mutate('CatalogService/CreateThread', { command: command(host.store), ...(workspace ? { project: workspace } : { environment: { host_id: hostId } }), agent_id: agent.agent_id, profile_id: account.account_id })); if (receipt.runtime?.catalog_id) router.push({ pathname: '/remote/chat', params: { hostId, chatId: receipt.runtime.catalog_id } }) }} />)}</React.Fragment>)}
      {state?.catalog?.threads.map(chat => <Action key={chat.catalog_id} title={`${chat.summary?.title} · ${chat.summary?.pending_decisions} decisions · ${chat.summary?.queued_messages} queued`} run={() => router.push({ pathname: '/remote/chat', params: { hostId, chatId: chat.catalog_id } })} />)}
      {state?.catalog?.terminals.map(terminal => <Action key={terminal.terminal?.terminal_id} title={terminal.title} run={() => router.push({ pathname: '/remote/terminal', params: { hostId, terminalId: terminal.terminal!.terminal_id } })} />)}
      <Field label="Project path on host" value={path} onChangeText={setPath} /><Action title="Open project" disabled={!path || state?.stale} run={async () => checkReceipt(await host.mutate('CatalogService/OpenProject', { command: command(host.store), environment: { host_id: hostId }, host_path: path }))} />
      <Action title="Remove connection" run={() => Alert.alert('Remove connection?', 'Local drafts and recovery records will be retained.', [{ text: 'Cancel' }, { text: 'Remove', style: 'destructive', onPress: () => { void remoteHosts().remove(hostId).then(() => router.back()).catch(e => Alert.alert('Could not remove', String(e))) } }])} />
    </>}
  </Page>
}
