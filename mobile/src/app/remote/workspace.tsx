import React, { useEffect, useMemo, useRef, useState } from 'react'
import { Alert } from 'react-native'
import { useLocalSearchParams, useRouter } from 'expo-router'
import { Action, Field, Page, useHost, useRevision } from '@/components/remote/common'
import { MediaPreview } from '@/components/remote/media'
import { ThemedText } from '@/components/themed-text'
import { command, checkReceipt } from '@/lib/remote/grpc/commands'
import { readTextFile, reloadTextFile, saveTextFile, previewFile } from '@/lib/remote/grpc/files'
import type { DirectoryEntries, GitStatus, GitHistory, GitRefs, PreflightRequest, GitPrecondition, CommandReceipt } from '@/lib/remote/grpc/schema'
import { fromBase64, toBase64 } from '@/lib/remote/grpc/protobuf'
export default function WorkspaceScreen() {
  const { hostId, workspaceId, filePath, showDiff } = useLocalSearchParams<{ hostId: string; workspaceId: string; filePath?: string; showDiff?: string }>(), { host, error } = useHost(hostId); useRevision(host)
  const controller = useMemo(() => new AbortController(), [hostId, workspaceId])
  useEffect(() => () => controller.abort(), [controller])
  const router = useRouter()
  const workspace = host?.snapshot().catalog?.projects.flatMap(project => project.checkouts).find(checkout => checkout.catalog_id === workspaceId)?.project
  const project = () => { if (!workspace || workspace.host_id !== hostId) throw Error('Checkout is unavailable; refresh the host catalog'); return workspace }
  const [directory, setDirectory] = useState(''), [entries, setEntries] = useState<DirectoryEntries>(), [query, setQuery] = useState(''), [path, setPath] = useState('')
  const [opened, setOpened] = useState(''), [text, setText] = useState(''), [hash, setHash] = useState<Uint8Array>(), [dirty, setDirty] = useState(false), [failure, setFailure] = useState('')
  const [git, setGit] = useState<GitStatus>(), [diff, setDiff] = useState(''), [message, setMessage] = useState(''), [remote, setRemote] = useState('origin'), [branch, setBranch] = useState(''), [history, setHistory] = useState<GitHistory>(), [refs, setRefs] = useState<GitRefs>()
  const textRef = useRef(''), newline = useRef<'lf' | 'crlf'>('lf')
  const [preview, setPreview] = useState<{ id: string; uri: string; mediaType: string; html?: string }>()
  useEffect(() => () => { if (preview) void host?.transport.native.removeAttachment(preview.id).catch(() => {}) }, [host, preview])
  const [upstream, setUpstream] = useState(''), [startPoint, setStartPoint] = useState(''), [targetBranch, setTargetBranch] = useState(''), [worktreePath, setWorktreePath] = useState('')
  const [approvals, setApprovals] = useState({ delete_branch: false, discard_changes: false, delete_unmerged_branch: false, acknowledge_offline_dirty: false })
  const checkResult = (receipt: CommandReceipt) => { checkReceipt(receipt); setFailure(receipt.git?.output ?? ''); return receipt }
  const ready = host?.snapshot().phase === 'ready' && !!workspace
  const list = async (dir = directory, more = false) => { if (host) { const result = await host.transport.unary('FilesService/ListDirectory', { directory: { target: { project: project() }, relative_path: dir }, page_token: more ? entries?.next_page_token : undefined }); setDirectory(dir); setEntries(more && entries ? { ...result, entries: [...entries.entries, ...result.entries] } : result) } }
  const refreshGit = async () => { if (host) setGit(await host.transport.unary('GitService/GetStatus', { project: project() })) }
  useEffect(() => { if (ready) { void list().catch(e => setFailure(String(e))); void refreshGit().catch(e => setFailure(String(e))) } }, [host, ready, hostId, workspaceId])
  useEffect(() => {
    if (!host || !opened || !hash || !ready) return
    const presence = () => host.transport.unary('FilesService/SetDirtyPresence', { command: command(host.store), file: { target: { project: project() }, relative_path: opened }, base_hash: hash, dirty })
    void presence().catch(e => setFailure(String(e)))
    const timer = setInterval(() => { if (dirty) void presence().catch(e => setFailure(String(e))) }, 15000)
    return () => clearInterval(timer)
  }, [host, opened, hash, dirty, ready])
  const open = async (value: string, reload = false) => {
    if (!host) return
    if (dirty && value !== opened && !reload) throw Error('Save or reload the current buffer before opening another file')
    const saved = host.store.snapshot().views?.[`file:${workspaceId}:${value}`]
    if (saved?.text !== undefined && saved.baseHash && !reload) { setText(saved.text); textRef.current = saved.text; newline.current = saved.newline ?? 'lf'; setHash(fromBase64(saved.baseHash)); setDirty(true) }
    else { const original = textRef.current
      const file = { target: { project: project() }, relative_path: value }
      const result = reload ? await reloadTextFile(host, file, controller.signal, () => textRef.current === original, `file:${workspaceId}:${value}`) : await readTextFile(host, file, controller.signal)
      if (controller.signal.aborted || textRef.current !== original) return
      setText(result.text); textRef.current = result.text; newline.current = result.text.includes('\r\n') ? 'crlf' : 'lf'; setHash(result.hash); setDirty(false) }
    setOpened(value)
  }
  useEffect(() => {
    if (!ready || !filePath || !host) return
    if (showDiff === 'true') void (async () => {
      let text = '', complete = false
      const stream = await host.transport.subscribe('GitService/ReadDiff', { project: project(), path: filePath, staged: false }, chunk => { text += chunk.lines.map(line => `${line.kind} ${line.text}`).join('\n') + '\n'; if (text.length > 8 * 1024 * 1024) throw Error('Diff exceeds view limit'); complete = chunk.complete }, controller.signal)
      await stream.done; if (!complete) throw Error('Incomplete diff'); if (!controller.signal.aborted) setDiff(text)
    })().catch(error => setFailure(String(error)))
    else void open(filePath).catch(error => setFailure(String(error)))
  }, [host, ready, filePath, showDiff])
  const preflight = async (intent: Omit<PreflightRequest, 'project'>): Promise<GitPrecondition> => {
    const target = project(), plan = await host!.transport.unary('GitService/Preflight', { project: target, ...intent })
    if (!plan.can_execute || !plan.expected || plan.project?.host_id !== hostId || plan.project.project_id !== target.project_id || plan.project.worktree_id !== target.worktree_id) throw Error(plan.reason || 'Git operation unavailable')
    await new Promise<void>((resolve, reject) => Alert.alert('Review Git operation', plan.impacts.join('\n') || 'Apply the selected operation?', [{ text: 'Cancel', onPress: () => reject(Error('Cancelled')) }, { text: 'Continue', onPress: () => resolve() }], { cancelable: false }))
    if (BigInt(plan.expires_unix) * 1000n <= BigInt(Date.now())) throw Error('Git plan expired; review again')
    return { ...plan.expected, preflight_id: plan.plan_id }
  }
  return <Page key={`${host?.snapshot().hostId}:workspace:${workspaceId}`} store={host?.store} viewKey={`workspace:${workspaceId}`} title="Workspace"><ThemedText>{error ?? failure}</ThemedText>{host && <>
    <Action title="Notes" run={() => router.push({ pathname: '/remote/notes', params: { hostId, workspaceId } })} />
    <Action title="Tasks" run={() => router.push({ pathname: '/remote/tasks', params: { hostId, workspaceId } })} />
    <Action title="Open terminal" disabled={!ready} run={async () => { const receipt = checkResult(await host.mutate('TerminalService/OpenTerminal', { command: command(host.store), project: project(), tool_id: 'terminal' })); if (receipt.terminal?.terminal_id) router.push({ pathname: '/remote/terminal', params: { hostId, terminalId: receipt.terminal.terminal_id } }) }} />
    <ThemedText>Files · {directory || '/'}</ThemedText><Action title="Parent directory" run={() => list(directory.split('/').slice(0, -1).join('/'))} />
    <Field label="Search files" value={query} onChangeText={setQuery} /><Action title="Search files" run={async () => setEntries(await host.transport.unary('FilesService/SearchFiles', { target: { project: project() }, query, limit: 200 }))} />
    {!!entries?.warning && <ThemedText>{entries.warning}</ThemedText>}{entries?.incomplete && <ThemedText>Directory listing is incomplete.</ThemedText>}
    {entries?.entries.map(entry => <React.Fragment key={entry.relative_path}><Action title={`${entry.directory ? '▸ ' : ''}${entry.name}`} run={() => entry.directory ? list(entry.relative_path) : open(entry.relative_path)} />{!entry.directory && <Action title={`Preview ${entry.name}`} run={async () => setPreview(await previewFile(host, { target: { project: project() }, relative_path: entry.relative_path }, controller.signal))} />}</React.Fragment>)}{entries?.next_page_token && <Action title="More files" run={() => list(directory, true)} />}
    {preview && <><MediaPreview uri={preview.uri} mediaType={preview.mediaType} html={preview.html} /><Action title="Close preview" run={() => setPreview(undefined)} /></>}
    <Field label="New relative path" value={path} onChangeText={setPath} />{[false, true].map(folder => <Action key={String(folder)} title={folder ? 'Create directory' : 'Create empty file'} disabled={!path || !ready} run={async () => { checkResult(await host.mutate('FilesService/CreateFile', { command: command(host.store), file: { target: { project: project() }, relative_path: path }, directory: folder })); await list() }} />)}
    {opened && <><ThemedText>{opened}{dirty ? ' · Unsaved' : ''}</ThemedText><Field label="File contents" multiline value={text} onChangeText={value => { setText(value); textRef.current = value; setDirty(true); void host.store.update(state => { (state.views ??= {})[`file:${workspaceId}:${opened}`] = { text: value, baseHash: hash && toBase64(hash), newline: newline.current }; return null }).catch(e => setFailure(String(e))) }} /><Action title="Save file" disabled={!ready || !dirty || !hash} run={async () => { const savedText = textRef.current, content = newline.current === 'crlf' ? savedText.replace(/\r?\n/g, '\r\n') : savedText;
      const result = await saveTextFile(host, { target: { project: project() }, relative_path: opened }, content, hash!);
      const newHash = result.file?.hash
      if (!newHash || newHash.length !== 32) throw Error('Save acknowledged without a new base hash; reload explicitly');
      setHash(newHash); const changedWhileSaving = textRef.current !== savedText; setDirty(changedWhileSaving);
      await host.store.update(state => { const views = state.views ??= {}; if (changedWhileSaving) views[`file:${workspaceId}:${opened}`] = { text: textRef.current, baseHash: toBase64(newHash), newline: newline.current }; else delete views[`file:${workspaceId}:${opened}`]; return null }) }} /><Action title="Reload from host" run={() => Alert.alert('Discard local edits?', 'The host version will replace this device’s draft.', [{ text: 'Cancel' }, { text: 'Reload', style: 'destructive', onPress: () => { void open(opened, true).catch(e => setFailure(String(e))) } }])} /></>}
    <ThemedText>Git · {git?.branch}</ThemedText><Action title="Refresh Git" run={refreshGit} />
    {git?.files.map(file => <React.Fragment key={file.path}><ThemedText>{file.path} · {file.index_status} / {file.worktree_status}</ThemedText>{[false, true].map(staged => <Action key={String(staged)} title={staged ? 'Staged diff' : 'Working diff'} run={async () => { let lines = '', complete = false, truncated = false; const stream = await host.transport.subscribe('GitService/ReadDiff', { project: project(), path: file.path, staged }, chunk => { if (complete) throw Error('Diff after completion'); lines += chunk.lines.map(l => `${l.kind} ${l.text}`).join('\n') + '\n'; if (lines.length > 8 * 1024 * 1024) throw Error('Diff exceeds view limit'); complete = chunk.complete; truncated ||= chunk.truncated }); await stream.done; if (!complete) throw Error('Incomplete diff'); setDiff(lines + (truncated ? '\nDiff truncated by host.' : '')) }} />)}{(['Stage', 'Unstage'] as const).map(method => <Action key={method} title={method} run={async () => { checkResult(await host.mutate(`GitService/${method}`, { command: command(host.store), project: project(), paths: [file.path], expected: await preflight({ paths: { operation: method === 'Stage' ? 1 : 2, paths: [file.path] } }) })); await refreshGit() }} />)}<Action title="Discard changes" run={async () => { const expected = await preflight({ paths: { operation: 3, paths: [file.path] } }); checkResult(await host.mutate('GitService/Discard', { command: command(host.store), project: project(), paths: [file.path], expected })); await refreshGit() }} /></React.Fragment>)}
    {!!diff && <ThemedText type="code" selectable>{diff}</ThemedText>}
    <Field label="Commit message" multiline value={message} onChangeText={setMessage} />{[false, true].map(sign => <Action key={String(sign)} title={sign ? 'Commit with signature' : 'Commit'} disabled={!message || !ready} run={async () => { checkResult(await host.mutate('GitService/Commit', { command: command(host.store), project: project(), message, sign, expected: await preflight({ commit: { message, sign } }) })); setMessage(''); await refreshGit() }} />)}
    <Field label="Upstream branch (optional)" value={upstream} onChangeText={setUpstream} /><Field label="Remote" value={remote} onChangeText={setRemote} />{(['Pull', 'Push'] as const).map(method => <Action key={method} title={method} run={async () => { checkResult(await host.mutate(`GitService/${method}`, { command: command(host.store), project: project(), remote, upstream_branch: upstream || undefined, expected: await preflight({ network: { action: method === 'Pull' ? 1 : 2, remote, upstream_branch: upstream || undefined } }) })); await refreshGit() }} />)}
    <Action title="History" run={async () => setHistory(await host.transport.unary('GitService/ReadHistory', { project: project(), limit: 100 }))} />{history?.commits.map(commit => <ThemedText key={commit.oid} selectable>{commit.title} · {commit.author} · {commit.oid}</ThemedText>)}{history?.next_before_oid && <Action title="Earlier commits" run={async () => { const page = await host.transport.unary('GitService/ReadHistory', { project: project(), limit: 100, before_oid: history.next_before_oid }); setHistory({ ...page, commits: [...history.commits, ...page.commits] }) }} />}
    <Action title="Branches and refs" run={async () => setRefs(await host.transport.unary('GitService/ListRefs', { project: project() }))} />{refs?.refs.map(ref => <ThemedText key={ref.name}>{ref.name} · {ref.oid}</ThemedText>)}
    <Field label="Worktree path (optional)" value={worktreePath} onChangeText={setWorktreePath} /><Field label="Start point (optional)" value={startPoint} onChangeText={setStartPoint} /><Field label="Merge target branch (optional)" value={targetBranch} onChangeText={setTargetBranch} />{([['Delete branch', 'delete_branch'], ['Discard changes', 'discard_changes'], ['Delete unmerged branch', 'delete_unmerged_branch'], ['Acknowledge offline unsaved buffers', 'acknowledge_offline_dirty']] as const).map(([label, field]) => <Action key={field} title={`${approvals[field] ? '✓ ' : ''}${label}`} run={() => setApprovals(value => ({ ...value, [field]: !value[field] }))} />)}
    <Field label="Worktree branch" value={branch} onChangeText={setBranch} />{([['Create worktree', 1], ['Remove worktree', 2], ['Merge worktree', 3], ['Stop worktree', 4]] as const).map(([title, action]) => <Action key={action} title={title} run={async () => { const worktree = { action, branch, path: worktreePath || undefined, start_point: startPoint || undefined, target_branch: targetBranch || undefined, ...approvals }; const expected = await preflight({ worktree }); checkResult(await host.mutate('GitService/ChangeWorktree', { command: command(host.store), project: project(), ...worktree, expected })); await refreshGit() }} />)}
  </>}</Page>
}
