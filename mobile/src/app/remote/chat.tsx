import { ComposerDraft } from '@/lib/remote/grpc/outbox'
import { EditMessage } from '@/components/remote/edit-message'
import { restoreChatRequest } from '@/lib/remote/grpc/chat-commands'
import { Transcript } from '@/components/remote/transcript'
import React, { useEffect, useLayoutEffect, useMemo, useRef, useState } from 'react'
import { timing, timingStart } from '@/lib/remote/grpc/metrics'
import { Alert, ScrollView, View } from 'react-native'
import { useLocalSearchParams, useRouter } from 'expo-router'
import { Action, Field, Page, useHost, useRevision } from '@/components/remote/common'
import { DecisionForm } from '@/components/remote/decision'
import { ArtifactPreview } from '@/components/remote/artifact'
import { ThemedText } from '@/components/themed-text'
import { ChatViewOwner } from '@/lib/remote/grpc/chat'
import { prepareUpload, upload } from '@/lib/remote/grpc/uploads'
import { fromBase64 } from '@/lib/remote/grpc/protobuf'
import { command, checkReceipt, uniqueId } from '@/lib/remote/grpc/commands'
import type { AgentOptions, SendContext, TranscriptItem, Attachment, HistoryMatches, ItemKey, AgentCatalog, PullRequests } from '@/lib/remote/grpc/schema'
import type { LocalImage } from '@/lib/remote/grpc/persistence'
import type { HostSession } from '@/lib/remote/grpc/hosts'
export default function ChatScreen() {
  const { hostId, chatId } = useLocalSearchParams<{ hostId: string; chatId: string }>(), { host, error } = useHost(hostId)
  return host ? <ChatView key={`${hostId}:${chatId}`} host={host} chatId={chatId} /> : <Page title="Chat"><ThemedText>{error ?? 'Loading…'}</ThemedText></Page>
}
function ChatView({ host, chatId }: { host: HostSession; chatId: string }) {
  const renderStarted = timingStart()
  useLayoutEffect(() => { timing(host.snapshot().hostId, chatId, undefined, 'render', renderStarted) })
  const router = useRouter()
  const scrollRef = useRef<ScrollView>(null), anchors = useRef(new Map<string, number>())
  const [anchor, setAnchor] = useState<string>(), [navigator, setNavigator] = useState(false)
  const [forkDestination, setForkDestination] = useState<string>()
  const [forkSelected, setForkSelected] = useState(false)
  const jump = async (key: ItemKey) => {
    const id = `${key.history_epoch}:${key.item_id}`
    setAnchor(id); await owner.jump(key)
    setThread(owner.snapshot().items.find(item => item.key?.history_epoch === key.history_epoch && item.key.item_id === key.item_id)?.parent_call_id)
    const y = anchors.current.get(id); if (y !== undefined) scrollRef.current?.scrollTo({ y, animated: false })
  }
  const searchAbort = useRef<AbortController | undefined>(undefined)
  useEffect(() => () => searchAbort.current?.abort(), [])
  const search = async (more = false) => {
    searchAbort.current?.abort(); const abort = new AbortController(); searchAbort.current = abort
    const result = await host.transport.unary('ThreadService/SearchHistory', { thread: chat, query, page_token: more ? matches?.next_page_token : undefined }, abort.signal)
    if (abort.signal.aborted) return
    setMatches(more && matches ? { ...result, incomplete: result.incomplete || matches.incomplete, matches: [...matches.matches, ...result.matches] } : result)
  }
  const owner = useMemo(() => new ChatViewOwner(host, chatId), [host, chatId])
  useRevision(owner); useRevision(host); useRevision(host.store)
  const view = owner.snapshot(), chat = owner.chat
  const listed = host.snapshot().catalog?.threads.find(c => c.catalog_id === chatId)
  const summary = view.summary ?? listed?.summary
  const project = host.snapshot().catalog?.projects.flatMap(p => p.checkouts).find(w => w.catalog_id === listed?.project_catalog_id)?.project
  const context: SendContext | undefined = summary && { account_id: summary.agent_id, profile_id: summary.profile_id, history_epoch: summary.history_epoch }
  const composer = useMemo(() => new ComposerDraft(host.outbox, chatId), [host, chatId])
  const [composed, setComposed] = useState(() => composer.snapshot())
  const draft = composed.draft.text, attachments = composed.draft.attachments, images = composed.images
  const [attachmentPath, setAttachmentPath] = useState(''), [commandName, setCommandName] = useState('')
  const editDraft = (patch: { text?: string; attachments?: Attachment[]; images?: LocalImage[] }, override?: SendContext) => {
    const saved = composer.edit(patch, override ?? composer.snapshot().context ?? context)
    setComposed(composer.snapshot())
    return saved
  }
  const [title, setTitle] = useState(''), [query, setQuery] = useState(''), [matches, setMatches] = useState<HistoryMatches>(), [options, setOptions] = useState<AgentOptions>()
  const [catalog, setCatalog] = useState<AgentCatalog>(), [pulls, setPulls] = useState<PullRequests>(), [pullQuery, setPullQuery] = useState('')
  const [shellMode, setShellMode] = useState(false)
  const [thread, setThread] = useState<string>()
  const [goal, setGoal] = useState(''), [optionError, setOptionError] = useState('')
  const supports = (operation: string) => options?.capabilities.some(c => c.operation === operation && c.supported) === true
  useEffect(() => { owner.start(); return () => owner.dispose() }, [owner])
  const ready = host.snapshot().phase === 'ready'
  useEffect(() => { if (ready) { const c = new AbortController(); void host.transport.unary('ThreadService/GetAgentOptions', { thread: chat }, c.signal).then(value => { setOptions(value); setOptionError('') }).catch(e => { if (!c.signal.aborted) setOptionError(String(e)) }); return () => c.abort() } }, [ready, host, chat])
  useEffect(() => {
    if (!ready || !summary?.agent_id || !summary.profile_id) return
    const abort = new AbortController()
    void host.transport.unary('CatalogService/GetAgentCatalog', { environment: { host_id: chat.host_id }, project, agent_id: summary.agent_id, account_id: summary.profile_id, refresh: false }, abort.signal).then(value => { if (!abort.signal.aborted) { setCatalog(value); setOptions(previous => previous?.options.length ? previous : { options: value.options, capabilities: value.capabilities }) } }).catch(error => { if (!abort.signal.aborted) setOptionError(String(error)) })
    return () => abort.abort()
  }, [host, ready, summary?.agent_id, summary?.profile_id, project?.project_id, project?.worktree_id])
  const send = async () => {
    const contextSaved = !composer.snapshot().context && context ? editDraft({}, context) : undefined
    const submitted = composer.snapshot()
    const { draft, images } = submitted
    const attachments = draft.attachments
    if (!submitted.context || (!draft.text.trim() && !images.length && !attachments.length)) return
    // Capture all fields before the first await; edits remain available during upload/RPC.
    await (contextSaved ?? composer.persist())
    const clear = async (alreadyCleared = false) => {
      if (await composer.clear(submitted, alreadyCleared)) setComposed(composer.snapshot())
    }
    if (shellMode) {
      if (!ready || !summary?.run_id) throw Error('Shell needs a connected running chat; text retained')
      if (attachments.length || images.length) throw Error('Shell does not accept attachments; switch back to message mode')
      const receipt = checkReceipt(await host.mutate('ThreadService/RunShell', { command: command(host.store), target: await host.threadTarget(chat), run_id: summary.run_id, command_line: draft.text }))
      if (receipt.state === 3) await clear()
      return
    }
    if (attachments.some(attachment => !attachment.image)) {
      if (!ready) throw Error('Contextual attachments require an online send; the draft is retained')
      const uploaded = [...attachments]
      for (const image of images) {
        const journal = host.store.snapshot().uploads?.find(u => u.localId === image.id && u.target.image?.thread?.thread_id === chat.thread_id)
          ?? await prepareUpload(host, { image: await host.threadTarget(chat) }, 'chat-image', image.id, image.size, fromBase64(image.sha256), image.mediaType)
        uploaded.push({ label: 'Image', image: await upload(host, journal.id) })
      }
      checkReceipt(await host.mutate('ThreadService/SendMessage', { command: command(host.store), target: await host.threadTarget(chat),
        message_id: uniqueId(), draft: { ...draft, attachments: uploaded }, expected_context: submitted.context }))
      await clear()
      return
    }
    const state = host.store.snapshot(), id = uniqueId()
    const cleared = await host.outbox.enqueue({ commandId: id, messageId: uniqueId(), hostId: chat.host_id, deviceId: state.connection.deviceId, chatId, context: submitted.context, draft, images, createdAt: new Date().toISOString() }, submitted.revision)
    await clear(cleared)
    if (ready) checkReceipt(await host.outbox.sendExplicit(id, host.transport))
  }
  const mutate = async (operation: 'ThreadService/Interrupt' | 'ThreadService/Restart') => checkReceipt(await host.mutate(operation, { command: command(host.store), target: await host.threadTarget(chat), run_id: summary?.run_id ?? '0' }))
  return <Page key={`${host?.snapshot().hostId}:chat:${chatId}`} store={host?.store} viewKey={`chat:${chatId}`} scrollRef={scrollRef} title={summary?.title ?? 'Chat'}>
    <ThemedText type="small" themeColor="textSecondary">{host.snapshot().phase} · {view.error ?? optionError}</ThemedText>
    <Action title="Load earlier history" run={() => owner.earlier()} disabled={!ready} />
    <Field label="Search saved history" value={query} onChangeText={value => { searchAbort.current?.abort(); setQuery(value); setMatches(undefined) }} /><Action title="Search" disabled={!ready || !query} run={() => search()} />
    {matches?.incomplete && <ThemedText>Some saved content is unavailable; these results are incomplete.</ThemedText>}
    {matches?.matches.map(match => <Action key={`${match.item?.history_epoch}:${match.item?.item_id}`} title={match.excerpt} disabled={!match.item} run={() => match.item && jump(match.item)} />)}
    {matches?.next_page_token && <Action title="More search results" run={() => search(true)} />}
    <Action title={navigator ? 'Hide transcript navigator' : 'Transcript navigator'} run={() => setNavigator(!navigator)} />
    {navigator && view.items.filter(item => item.user || item.turn_end).map(item => <Action key={`${item.key?.history_epoch}:${item.key?.item_id}`} title={item.user?.draft?.text.slice(0, 100) || item.turn_end?.outcome || 'Turn'} disabled={!item.key} run={() => item.key && jump(item.key)} />)}
    <Action title="Live conversation" run={() => { setAnchor(undefined); owner.live() }} />
    <ThemedText>Fork destination</ThemedText><Action title={`${forkSelected && !forkDestination ? '✓ ' : ''}Private chat`} run={() => { setForkSelected(true); setForkDestination(undefined) }} />
    {host.snapshot().catalog?.projects.flatMap(project => project.checkouts.filter(checkout => !checkout.closing).map(checkout => <Action key={checkout.catalog_id} title={`${forkDestination === checkout.catalog_id ? '✓ ' : ''}${project.name} / ${checkout.name}`} run={() => { setForkSelected(true); setForkDestination(checkout.catalog_id) }} />))}
    {thread && <Action title="Back to conversation" run={() => setThread(undefined)} />}
    {view.items.filter(item => thread ? item.parent_call_id === thread : !item.parent_call_id).map(item => <View key={`${item.key?.history_epoch}:${item.key?.item_id}`} onLayout={event => { const id = `${item.key?.history_epoch}:${item.key?.item_id}`; anchors.current.set(id, event.nativeEvent.layout.y); if (anchor === id) scrollRef.current?.scrollTo({ y: event.nativeEvent.layout.y, animated: false }) }}><Transcript item={item} host={host} chat={chat} />
      {item.shell?.running && item.key && <Action title="Stop shell" run={async () => checkReceipt(await host.mutate('ThreadService/StopShell', { command: command(host.store), target: await host.threadTarget(chat), run_id: summary?.run_id ?? '', item: item.key }))} />}
      {item.user && item.key && <Action title="Edit sent message…" disabled={!ready || !supports('edit')} run={async () => {
        if (host.store.snapshot().edits?.[chatId]) throw Error('Finish or cancel the current edit first; its draft is retained')
        const preview = await host.transport.unary('ThreadService/PreviewEdit', { thread: chat, run_id: summary?.run_id ?? '', item: item.key })
        if (!preview.editable || !preview.original) throw Error(preview.reason || 'Original message is unavailable')
        await host.store.update(state => { (state.edits ??= {})[chatId] = { preview, draft: preview.original!, restoreFiles: false }; return null })
      }} />}{item.tool && view.items.some(child => child.parent_call_id === item.tool!.call_id) && <Action title="Open subagent thread" run={() => setThread(item.tool!.call_id)} />}<Action title="Quote" disabled={!supports('embedded_context')} run={async () => { const quote = item.text?.text ?? item.user?.draft?.text ?? item.tool?.output ?? item.thinking?.text; if (!quote || !context) return; const next = [...composer.snapshot().draft.attachments, { label: 'Quote', quote: { text: quote } }]; await editDraft({ attachments: next }) }} />{item.user?.draft?.attachments.map((attachment, index) => attachment.image ? <ArtifactPreview key={index} host={host} artifact={attachment.image} /> : <ThemedText key={index}>{attachment.label}</ThemedText>)}{item.decision?.state === 'pending' && <DecisionForm key={`${item.decision.run_id}:${item.decision.decision_id}`} host={host} chat={chat} decision={item.decision} ready={ready} />}{item.tool?.background && <Action title="Stop background task" run={async () => checkReceipt(await host.mutate('ThreadService/StopBackground', { command: command(host.store), target: await host.threadTarget(chat), run_id: summary?.run_id ?? '', call_id: item.tool!.call_id }))} />}{item.key && item.turn_end && !item.trimmed && item.turn_end.fork_available === true && supports('fork') && <Action title="Fork here" disabled={!ready} run={async () => {
        const destination = forkSelected ? forkDestination : listed?.project_catalog_id
        if (!destination || destination !== listed?.project_catalog_id) throw Error('Remote Fork requires the source project and checkout; Private or another checkout is unavailable')
        if (!host.snapshot().catalog?.projects.some(project => project.checkouts.some(checkout => checkout.catalog_id === destination && !checkout.closing))) throw Error('Fork destination is closing or unavailable')
        const receipt = checkReceipt(await host.mutate('ThreadService/ForkThread', { command: command(host.store), target: await host.threadTarget(chat), history_epoch: item.key!.history_epoch, item_id: item.key!.item_id }))
        if (receipt.runtime?.catalog_id) router.push({ pathname: '/remote/chat', params: { hostId: chat.host_id, chatId: receipt.runtime.catalog_id } })
      }} />}</View>)}
    {host.store.snapshot().edits?.[chatId] && <EditMessage key={`${host.store.snapshot().edits![chatId].preview.item?.history_epoch}:${host.store.snapshot().edits![chatId].preview.item?.item_id}`} host={host} chat={chat} chatId={chatId} runId={summary?.run_id ?? ''} />}
    {host.store.snapshot().mutations?.filter(record => record.chatRequest && (() => { try { return restoreChatRequest(record.method, record.chatRequest).target?.thread?.thread_id === chat.thread_id } catch { return true } })()).map(record => <React.Fragment key={record.commandId}><ThemedText>{record.method.split('/')[1]} · {record.receipt?.error?.message ?? `State ${record.receipt?.state ?? 'unknown'}`}</ThemedText><Action title="Query / retry original operation" disabled={!ready} run={async () => { const receipt = checkReceipt(await host.retryChatMutation(record.commandId)); if (receipt.runtime?.catalog_id && record.method === 'ThreadService/ForkThread') router.push({ pathname: '/remote/chat', params: { hostId: chat.host_id, chatId: receipt.runtime.catalog_id } }) }} /></React.Fragment>)}
    <Action title={shellMode ? 'Mode: shell · switch to message' : 'Mode: message · switch to shell'} disabled={!supports('shell')} run={() => setShellMode(!shellMode)} />
    {shellMode && <ThemedText>Runs on the host in this chat’s working directory. Message mode sends “!” literally.</ThemedText>}
    <Field label={shellMode ? 'Shell command' : 'Message'} multiline value={draft} onChangeText={text => { void editDraft({ text }).catch(e => Alert.alert('Draft not saved', String(e))) }} />
    {attachments.map((attachment, index) => <Action key={index} title={`Remove ${attachment.label}`} run={async () => { const next = composer.snapshot().draft.attachments.filter((_, i) => i !== index); if (context) await editDraft({ attachments: next }) }} />)}
    {project && <><Field label="Host file path relative to workspace" value={attachmentPath} onChangeText={setAttachmentPath} /><Action title="Attach host path" disabled={!attachmentPath || !supports('embedded_context')} run={async () => { const next = [...composer.snapshot().draft.attachments, { label: attachmentPath, file: { project_file: true, relative_path: attachmentPath } }]; await editDraft({ attachments: next }); setAttachmentPath('') }} /></>}
    <Field label="Find commands and skills" value={commandName} onChangeText={setCommandName} />
    {catalog?.error && <ThemedText>{catalog.error}</ThemedText>}
    <Action title="Refresh commands and models" disabled={!ready || !summary?.profile_id} run={async () => { const value = await host.transport.unary('CatalogService/GetAgentCatalog', { environment: { host_id: chat.host_id }, project, agent_id: summary!.agent_id, account_id: summary!.profile_id!, refresh: true }); setCatalog(value) }} />
    {catalog?.commands.filter(value => `${value.name} ${value.description}`.toLowerCase().includes(commandName.toLowerCase())).map(value => <Action key={value.id} title={`${value.skill ? 'Skill' : 'Command'}: ${value.name} · ${value.description}${value.argument_hint ? ` (${value.argument_hint})` : ''}`} disabled={!context || !supports('embedded_context')} run={async () => { const next = [...composer.snapshot().draft.attachments, { label: value.name, command: { name: value.name, skill: value.skill, path: value.path } }]; await editDraft({ attachments: next }) }} />)}
    {project && <><Field label="Find pull requests" value={pullQuery} onChangeText={value => { setPullQuery(value); setPulls(undefined) }} /><Action title="Find pull requests" disabled={!ready} run={async () => setPulls(await host.transport.unary('CatalogService/ListPullRequests', { project, query: pullQuery }))} />
      {pulls?.pulls.map(choice => <Action key={choice.context_id} title={`${choice.pull?.title ?? choice.context_id} · ${choice.author}`} disabled={!choice.pull} run={async () => { const next = [...composer.snapshot().draft.attachments, { label: choice.pull!.title, pull: { ...choice.pull!, context_id: choice.context_id } }]; await editDraft({ attachments: next }) }} />)}
      {pulls?.next_page_token && <Action title="More pull requests" run={async () => { const next = await host.transport.unary('CatalogService/ListPullRequests', { project, query: pullQuery, page_token: pulls.next_page_token }); setPulls({ ...next, pulls: [...pulls.pulls, ...next.pulls] }) }} />}
    </>}
    {images.map(image => <React.Fragment key={image.id}><ThemedText type="small">Image · {(image.size / 1024 / 1024).toFixed(1)} MiB</ThemedText><Action title="Remove image" run={async () => { const next = composer.snapshot().images.filter(i => i.id !== image.id); if (context) await editDraft({ images: next }) }} /></React.Fragment>)}
    <Action title="Attach image" disabled={!supports('images') || images.length >= 10 || !context} run={async () => { const image = await host.transport.native.pickImage(); const next = [...composer.snapshot().images, image]; await editDraft({ images: next }) }} />
    <Action title={shellMode ? 'Run shell' : ready ? 'Send' : 'Keep in outbox'} disabled={!context || (!draft.trim() && !images.length && !attachments.length)} run={send} />
    {host.store.snapshot().outbox.filter(e => e.chatId === chatId).map(entry => <React.Fragment key={entry.commandId}><ThemedText selectable>{entry.draft.text}</ThemedText><ThemedText type="small">{entry.receipt?.error?.message ?? (entry.receipt?.state === 6 ? 'Delivery unknown — review queue before deciding' : entry.receipt ? `Result ${entry.receipt.state}` : entry.epoch ? 'Awaiting reconciliation' : 'Not sent')}</ThemedText><Action title="Reconcile / send explicitly" disabled={!ready || entry.receipt?.state === 6} run={async () => checkReceipt(await host.outbox.sendExplicit(entry.commandId, host.transport))} />{entry.epoch === undefined && <Action title="Confirm current chat context and send" disabled={!ready} run={() => new Promise<void>(resolve => Alert.alert('Use current context?', 'This message will use the current history and workspace. Its account must still match.', [{ text: 'Cancel', onPress: () => resolve() }, { text: 'Send', onPress: () => { void host.outbox.sendExplicit(entry.commandId, host.transport, context).then(checkReceipt).then(() => resolve()).catch(e => { Alert.alert('Not sent', String(e)); resolve() }) } }]))} />}{entry.receipt && [4,5,7].includes(entry.receipt.state) && <Action title="Restore as a new draft" run={async () => { await editDraft({ text: entry.draft.text, attachments: entry.draft.attachments, images: entry.images ?? [] }, entry.context) }} />}<Action title="Dismiss confirmed result / unsent message" run={() => host.outbox.dismiss(entry.commandId)} /></React.Fragment>)}
    {view.queue && <><ThemedText>Host queue</ThemedText><Action title={view.queue.paused ? 'Resume queue' : 'Pause queue'} disabled={!ready} run={async () => checkReceipt(await host.mutate('ThreadService/PauseQueue', { command: command(host.store), target: await host.threadTarget(chat), queue_revision: view.queue!.revision, paused: !view.queue!.paused }))} />{view.queue.entries.map(entry => <React.Fragment key={entry.message_id}><ThemedText>{entry.draft?.text} · {entry.delivery}</ThemedText>{entry.delivery === 6 ? ([['Accept as delivered', 1], ['Send again', 2], ['Dismiss unknown', 3]] as const).map(([label, resolution]) => <Action key={resolution} title={label} run={async () => checkReceipt(await host.mutate('ThreadService/ResolveUnknownQueueMessage', { command: command(host.store), target: await host.threadTarget(chat), queue_revision: view.queue!.revision, message_id: entry.message_id, resolution, new_message_id: resolution === 2 ? uniqueId() : undefined }))} />) : <><Action title="Replace with composer text" run={async () => checkReceipt(await host.mutate('ThreadService/EditQueueMessage', { command: command(host.store), target: await host.threadTarget(chat), queue_revision: view.queue!.revision, message_id: entry.message_id, draft: { text: draft, attachments: entry.draft?.attachments ?? [], goal: entry.draft?.goal ?? false } }))} /><Action title="Move to end" run={async () => checkReceipt(await host.mutate('ThreadService/MoveQueueMessage', { command: command(host.store), target: await host.threadTarget(chat), queue_revision: view.queue!.revision, message_id: entry.message_id }))} /><Action title="Remove queued message" run={async () => checkReceipt(await host.mutate('ThreadService/RemoveQueueMessage', { command: command(host.store), target: await host.threadTarget(chat), queue_revision: view.queue!.revision, message_id: entry.message_id }))} /></>}</React.Fragment>)}</>}
    {options?.options.map(option => <React.Fragment key={option.id}><ThemedText>{option.label} · {option.effect}</ThemedText>{option.choices.map(choice => <Action key={choice.id} title={`${choice.id === option.current_value ? '✓ ' : ''}${choice.label}`} disabled={!ready} run={async () => { checkReceipt(await host.mutate('ThreadService/SetAgentOption', { command: command(host.store), target: await host.threadTarget(chat), option_id: option.id, value_id: choice.id })); setOptions(await host.transport.unary('ThreadService/GetAgentOptions', { thread: chat })) }} />)}</React.Fragment>)}
    <Action title="Steer with composer" disabled={!ready || !supports('steer') || !draft || !summary?.run_id} run={async () => checkReceipt(await host.mutate('ThreadService/Steer', { command: command(host.store), target: await host.threadTarget(chat), run_id: summary!.run_id!, message_id: uniqueId(), draft: { text: draft, attachments: [], goal: false } }))} />
    <Action title="Interrupt" disabled={!ready || !supports('cancel')} run={() => mutate('ThreadService/Interrupt')} /><Action title="Restart" disabled={!ready} run={() => mutate('ThreadService/Restart')} />
    <Field label="Goal objective" value={goal} onChangeText={setGoal} /><Action title="Set goal" disabled={!ready || !supports('goal')} run={async () => checkReceipt(await host.mutate('ThreadService/SetGoal', { command: command(host.store), target: await host.threadTarget(chat), objective: goal }))} />
    {summary?.goal && <><ThemedText>{summary.goal.objective} · {summary.goal.state}</ThemedText><ThemedText>Turns: {summary.goal.turns} · Tokens: {summary.goal.tokens ?? '—'} · Budget: {summary.goal.budget ?? '—'} · Elapsed: {summary.goal.elapsed_seconds ?? '—'}s · Checks: {summary.goal.checks ?? '—'}</ThemedText>{summary.goal.note && <ThemedText>{summary.goal.note}</ThemedText>}</>}
    {([['Pause goal', 'PauseGoal'], ['Resume goal', 'ResumeGoal'], ['Clear goal', 'ClearGoal']] as const).map(([label, action]) => <Action key={action} title={label} disabled={!ready || !supports('goal') || !summary?.goal} run={async () => checkReceipt(await host.mutate(`ThreadService/${action}`, { command: command(host.store), target: await host.threadTarget(chat) }))} />)}

    <Field label="Chat title" value={title} onChangeText={setTitle} /><Action title="Rename" disabled={!ready || !title} run={async () => checkReceipt(await host.mutate('CatalogService/RenameThread', { command: command(host.store), target: await host.threadTarget(chat), title }))} />
    <Action title="Close chat view" run={() => { owner.dispose(); router.back() }} />
    <Action title="Delete chat…" disabled={!ready} run={async () => {
      const plan = await host.transport.unary('CatalogService/PreflightClose', { thread: chat })
      if (!plan.can_execute) throw Error(plan.reason)
      const target = await host.threadTarget(chat)
      Alert.alert('Delete chat?', ['This deletes the chat on the host for every device.', ...plan.risks.map(r => `${r.title}: ${r.queued} queued, ${r.observers} viewers`)].join('\n'), [{ text: 'Cancel' }, { text: 'Delete', style: 'destructive', onPress: () => { void host.mutate('CatalogService/CloseThread', { command: command(host.store), target, plan_id: plan.plan_id }).then(checkReceipt).then(() => { owner.dispose(); router.back() }).catch(e => Alert.alert('Could not delete', String(e))) } }])
    }} />
  </Page>
}
