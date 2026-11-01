import React, { useState } from 'react'
import { Alert } from 'react-native'
import { useLocalSearchParams } from 'expo-router'
import { Action, Field, Page, useHost, useRevision } from '@/components/remote/common'
import { Markdown } from '@/components/remote/markdown'
import { ThemedText } from '@/components/themed-text'
import { readTextFile, saveTextFile } from '@/lib/remote/grpc/files'
import { command, checkReceipt } from '@/lib/remote/grpc/commands'
// Same ephemeral lifecycle as the local Notes pane: retained until client exit.
const notes = new Map<string, string>()
export default function NotesScreen() {
  const { hostId, workspaceId } = useLocalSearchParams<{ hostId: string; workspaceId: string }>()
  const { host, error } = useHost(hostId); useRevision(host)
  const [scope, setScope] = useState<'worktree' | 'project'>('worktree'), [preview, setPreview] = useState(false), [, redraw] = useState(0), [path, setPath] = useState('')
  const project = host?.snapshot().catalog?.projects.find(project => project.checkouts.some(checkout => checkout.catalog_id === workspaceId))
  const checkout = project?.checkouts.find(checkout => checkout.catalog_id === workspaceId)
  const key = JSON.stringify([hostId, scope, scope === 'project' ? project?.path : workspaceId])
  const text = notes.get(key) ?? ''
  const change = (value: string) => { notes.set(key, value); redraw(value => value + 1) }
  const exportNote = async (create: boolean) => {
    if (!host || !path.trim()) return
    if (!checkout?.project) throw Error('Checkout is unavailable')
    const file = { target: { project: checkout.project }, relative_path: path }
    const source = text
    if (create) checkReceipt(await host.mutate('FilesService/CreateFile', { command: command(host.store), file, directory: false }))
    const current = await readTextFile(host, file)
    if (!create) await new Promise<void>((resolve, reject) => Alert.alert(`Replace ${path}?`, 'The exported note will replace this host file. A concurrent change will be rejected.', [{ text: 'Cancel', onPress: () => reject(Error('Export cancelled; note retained')) }, { text: 'Replace', onPress: () => resolve() }], { cancelable: false }))
    await saveTextFile(host, file, source, current.hash)
    Alert.alert('Note exported', path)
  }
  return <Page title="Notes"><ThemedText>{error}</ThemedText><ThemedText type="small">Notes stay on this device until the app exits. Export to keep a file on the host.</ThemedText>
    {(['worktree', 'project'] as const).map(value => <Action key={value} title={`${scope === value ? '✓ ' : ''}${value}`} disabled={value === 'project' && !project} run={() => setScope(value)} />)}
    <Field label="Markdown note" multiline value={text} onChangeText={change} />
    <Action title={preview ? 'Hide preview' : 'Show editable preview'} run={() => setPreview(!preview)} />
    {preview && <Markdown text={text} change={change} />}
    <Field label="Export path relative to checkout" value={path} onChangeText={setPath} />
    <Action title="Export as new file" disabled={!host || !path} run={() => exportNote(true)} /><Action title="Export to existing file…" disabled={!host || !path} run={() => exportNote(false)} />
  </Page>
}
