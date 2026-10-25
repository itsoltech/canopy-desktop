import React, { useEffect, useRef, useState } from 'react'
import { Alert } from 'react-native'
import { useLocalSearchParams } from 'expo-router'
import { Action, Field, Page, useHost, useRevision } from '@/components/remote/common'
import { useTaskDraft } from '@/components/remote/task-draft'
import { TaskFieldControl } from '@/components/remote/task-field'
import { TaskAttachmentPreview } from '@/components/remote/task-attachment'
import { prepareUpload, upload } from '@/lib/remote/grpc/uploads'
import { fromBase64 } from '@/lib/remote/grpc/protobuf'
import { ArtifactPreview } from '@/components/remote/artifact'
import { ThemedText } from '@/components/themed-text'
import { command, checkReceipt, uniqueId } from '@/lib/remote/grpc/commands'
import { plain, validateFields, valueOf } from '@/lib/remote/grpc/values'
import type { TaskIntegration, Task, TaskPage, TaskChoicePage, TaskSchema, TaskCommentPage, TaskMutationRequest, TaskStatusChoice, Value, TaskSchemaPurpose, TaskProject } from '@/lib/remote/grpc/schema'
export default function TasksScreen() {
  const { hostId, workspaceId } = useLocalSearchParams<{ hostId: string; workspaceId: string }>(), { host, error } = useHost(hostId)
  useRevision(host?.store)
  const workspace = workspaceId ? { host_id: hostId, workspace_id: workspaceId } : undefined
  const [integrations, setIntegrations] = useState<TaskIntegration[]>([]), [selected, setSelected] = useState(''), [projects, setProjects] = useState<TaskChoicePage>(), [projectId, setProjectId] = useState('')
  const [query, setQuery] = useState(''), [state, setState] = useState('open'), [filterId, setFilterId] = useState<string>(), [filterName, setFilterName] = useState(''), [expression, setExpression] = useState('')
  const [page, setPage] = useState<TaskPage>(), [task, setTask] = useState<Task>()
  const [failure, setFailure] = useState('')
  const target = JSON.stringify([workspaceId, selected, projectId, task?.task?.task_id ?? 'new'])
  const targetRef = useRef(target); targetRef.current = target
  const useDraftField = <T,>(field: string, value: T) => useTaskDraft(host?.store, target, field, value, setFailure)
  const [branch, setBranch] = useDraftField('branch', '')
  const [schema, setSchema] = useDraftField<TaskSchema | undefined>('schema', undefined), [fields, setFields] = useDraftField<Record<string, Value>>('fields', {}), [purpose, setPurpose] = useDraftField<TaskSchemaPurpose>('purpose', 2), [status, setStatus] = useDraftField<TaskStatusChoice | undefined>('status', undefined)
  const [comments, setComments] = useState<TaskCommentPage>(), [comment, setComment] = useDraftField('comment', ''), [commentId, setCommentId] = useDraftField<string | undefined>('commentId', undefined)
  const [options, setOptions] = useState<TaskChoicePage>(), [optionKind, setOptionKind] = useState('label'), [title, setTitle] = useDraftField('title', task?.title ?? ''), [description, setDescription] = useDraftField('description', task?.description ?? '')
  const [otherTask, setOtherTask] = useDraftField('otherTask', ''), [linkKind, setLinkKind] = useDraftField('linkKind', ''), [seconds, setSeconds] = useDraftField('seconds', ''), [started, setStarted] = useDraftField('started', ''), [confirmation, setConfirmation] = useState(''), [boardId, setBoardId] = useDraftField('boardId', '')
  const project: TaskProject = { host_id: hostId, integration_id: selected, project_id: projectId, workspace }
  const provider = integrations.find(i => i.id === selected)?.provider
  useEffect(() => { if (host) { const abort = new AbortController(); void host.transport.unary('TasksService/ListIntegrations', { host_id: hostId, workspace }, abort.signal).then(value => setIntegrations(value.integrations)).catch(e => setFailure(String(e))); return () => abort.abort() } }, [host, hostId, workspaceId])
  const search = async (more = false) => { if (host) { const result = await host.transport.unary('TasksService/QueryTasks', { host_id: hostId, integration_id: selected, project_id: projectId, workspace, query, state, filter_id: filterId, filter_expression: expression || undefined, page_token: more ? page?.next_page_token : undefined }); setPage(more && page ? { ...result, tasks: [...page.tasks, ...result.tasks] } : result) } }
  const [schemaRequest, setSchemaRequest] = useDraftField<{ issueType?: string; board?: string }>('schemaRequest', {})
  const loadSchema = async (purpose: TaskSchemaPurpose, issueType?: string, board?: string) => {
    if (!host) return
    const next = await host.transport.unary('TasksService/GetSchema', { project, task: purpose === 1 ? undefined : task?.task, purpose, issue_type_id: issueType, board_id: board })
    if (targetRef.current !== target) return
    if (purpose === 1 && task) {
      const createTarget = JSON.stringify([workspaceId, selected, projectId, 'new'])
      await host.store.update(state => { const record = ((state.taskDrafts ??= {})[createTarget] ??= {}); record.schema = next; record.purpose = purpose; record.schemaRequest = { issueType, board }; return null })
      setTask(undefined)
    } else { setSchema(next); setSchemaRequest({ issueType, board }); setPurpose(purpose) }
  }
  const refreshTask = async () => { if (host && task?.task) { const value = await host.transport.unary('TasksService/GetTask', { task: task.task }); if (targetRef.current === target) setTask(value) } }
  const mutate = async (action: Omit<TaskMutationRequest, 'command' | 'project' | 'task'>) => { if (!host) return; checkReceipt(await host.mutate('TasksService/ApplyMutation', { command: command(host.store), project, task: task?.task, ...action })); await refreshTask() }
  const readComments = async (more = false) => { if (host) { const result = await host.transport.unary('TasksService/ReadComments', { task: task?.task, page_token: more ? comments?.next_page_token : undefined }); setComments(more && comments ? { ...result, comments: [...comments.comments, ...result.comments] } : result) } }
  return <Page key={`${host?.snapshot().hostId}:tasks:${workspaceId}`} store={host?.store} viewKey={`tasks:${workspaceId}`} title="Tasks"><ThemedText>{error ?? failure}</ThemedText>{host && <>
    {Object.entries(host.store.snapshot().taskDrafts ?? {}).filter(([key]) => { try { return JSON.parse(key)[0] === workspaceId } catch { return false } }).map(([key, saved]) => <Action key={key} title={`Restore draft · ${String(saved.title || saved.comment || 'Task form').slice(0, 100)}`} run={async () => {
      const [, integration, project, taskId] = JSON.parse(key) as string[]
      const restored = taskId === 'new' ? undefined : await host.transport.unary('TasksService/GetTask', { task: { host_id: hostId, integration_id: integration, project_id: project, task_id: taskId, workspace } })
      setSelected(integration); setProjectId(project); setTask(restored)
      setProjects(await host.transport.unary('TasksService/ListProjects', { host_id: hostId, integration_id: integration, workspace }))
    }} />)}
    {integrations.map(integration => <Action key={integration.id} title={`${selected === integration.id ? '✓ ' : ''}${integration.name}`} disabled={!integration.configured} run={async () => { setSelected(integration.id); setProjectId(''); setTask(undefined); setPage(undefined); setProjects(await host.transport.unary('TasksService/ListProjects', { host_id: hostId, integration_id: integration.id, workspace })) }} />)}
    {projects?.choices.map(choice => <Action key={choice.id} title={`${projectId === choice.id ? '✓ ' : ''}${choice.label}`} run={() => { setProjectId(choice.id); setTask(undefined); setPage(undefined) }} />)}{projects?.next_page_token && <Action title="More projects" run={async () => { const next = await host.transport.unary('TasksService/ListProjects', { host_id: hostId, integration_id: selected, workspace, page_token: projects.next_page_token }); setProjects({ ...next, choices: [...projects.choices, ...next.choices] }) }} />}
    <Field label="Search tasks" value={query} onChangeText={setQuery} />{['open', 'closed'].map(value => <Action key={value} title={`${state === value ? '✓ ' : ''}${value}`} run={() => setState(value)} />)}<Action title="Search" disabled={!projectId} run={() => search()} />
    {page?.filters.map(filter => <React.Fragment key={filter.id}><Action title={`${filterId === filter.id ? '✓ ' : ''}${filter.name}`} run={() => { setFilterId(filter.id); setExpression(filter.expression) }} />{!filter.builtin && <Action title={`Remove ${filter.name}`} run={async () => { checkReceipt(await host.mutate('TasksService/RemoveFilter', { command: command(host.store), filter_id: filter.id })); await search() }} />}</React.Fragment>)}
    {provider && provider !== 'github' && <><Field label="Provider filter expression" value={expression} onChangeText={setExpression} /><Field label="Saved filter name" value={filterName} onChangeText={setFilterName} /><Action title="Save filter" disabled={!filterName || !provider} run={async () => { checkReceipt(await host.mutate('TasksService/SaveFilter', { command: command(host.store), filter: { id: uniqueId(), name: filterName, expression, provider: provider!, builtin: false } })); await search() }} /></>}
    {page?.tasks.map(value => <Action key={`${value.task?.project_id}:${value.task?.task_id}`} title={`${value.title} · ${value.state}`} run={async () => { const result = await host.transport.unary('TasksService/GetTask', { task: value.task }); setTask(result); setComments(undefined) }} />)}{page?.next_page_token && <Action title="More tasks" run={() => search(true)} />}
    <Action title="Choose task type" disabled={!projectId} run={() => loadSchema(4)} />{purpose === 4 && schema?.choices.map(choice => <Action key={choice.id} title={choice.label} run={async () => { await loadSchema(1, choice.id) }} />)}
    <Action title="Create task" disabled={!projectId} run={async () => { await loadSchema(1) }} />
      {schema?.next_page_token && <Action title="More activity or schema choices" run={async () => { const next = await host.transport.unary('TasksService/GetSchema', { project, task: purpose === 1 ? undefined : task?.task, purpose, issue_type_id: schemaRequest.issueType, board_id: schemaRequest.board, page_token: schema.next_page_token }); setSchema({ ...next, fields: [...schema.fields, ...next.fields], choices: [...schema.choices, ...next.choices], transitions: [...schema.transitions, ...next.transitions], activity: [...schema.activity, ...next.activity] }) }} />}
    {(task || purpose === 1 && schema) && <><Field label="Task title" value={title} onChangeText={setTitle} /><Field label="Description" multiline value={description} onChangeText={setDescription} />
      {task && <><ThemedText>State: {task.state}</ThemedText><ThemedText selectable>{task.url}</ThemedText>{task.capabilities.filter(c => !c.supported).map(c => <ThemedText key={c.operation} type="small">{c.operation}: {c.reason}</ThemedText>)}<Action title="Save title and description" run={async () => { checkReceipt(await host.mutate('TasksService/UpdateTask', { command: command(host.store), task: task.task, title, description })); await refreshTask() }} />
      {task.statuses.map(field => <React.Fragment key={field.id}><ThemedText>{field.name} · {field.hint ?? field.label}</ThemedText>{field.read_only_reason ? <ThemedText>{field.read_only_reason}</ThemedText> : field.choices.map(choice => <Action key={choice.id} title={`${field.selected_id === choice.id ? '✓ ' : ''}${choice.label}`} run={async () => { const transitions = await host.transport.unary('TasksService/GetSchema', { project, task: task.task, purpose: 3 }); setSchema({ ...transitions, fields: transitions.transitions.find(t => t.id === choice.id)?.required_fields ?? [] }); setFields({}); setStatus({ field_id: field.id, choice_id: choice.id }); setPurpose(3) }} />)}</React.Fragment>)}
      <Action title="Edit provider fields" run={() => loadSchema(2)} /><Action title="Activity" run={() => loadSchema(8)} />{task.fields.map(field => <TaskFieldControl key={field.id} field={field} value={fields[field.id]} change={value => setFields(current => ({ ...current, [field.id]: value }))} />)}<Action title="Save provider fields" disabled={!Object.keys(fields).length} run={() => mutate({ fields: validateFields(task.fields, fields) })} />
      </>}
      {schema?.fields.map(field => <TaskFieldControl key={field.id} field={field} value={fields[field.id]} change={value => setFields(current => ({ ...current, [field.id]: value }))} />)}
      {status && <Action title="Apply workflow choice" run={async () => { await mutate({ status: { ...status, form_fields: validateFields(schema?.fields ?? [], fields) } }); setStatus(undefined); setSchema(undefined) }} />}
      {purpose === 1 && schema && !task && <Action title="Create" disabled={!title} run={async () => { await mutate({ create: { title, description, labels: [], assignees: [], fields: validateFields(schema.fields, fields) } }); await search() }} />}

      {schema?.activity.map((line, i) => <ThemedText key={i}>{line}</ThemedText>)}
    </>}
    {task && <>
      {task.links.map(link => <Action key={link.id} title={`Remove link: ${link.label || link.other_task}`} run={() => mutate({ link: { kind: link.kind, other_task: link.other_task, outward: link.outward, delete_link_id: link.id } })} />)}
      {(['label', 'assignee', 'milestone'] as const).map(kind => <Action key={kind} title={`Choose ${kind}`} run={async () => { setOptionKind(kind); setOptions(await host.transport.unary('TasksService/ReadOptions', { project, kind })) }} />)}
      {options?.choices.map(choice => <Action key={choice.id} title={choice.label} run={() => mutate(optionKind === 'label' ? { tag: { id: choice.id, add: !task.labels.includes(choice.id) } } : optionKind === 'assignee' ? { assignee: { id: choice.id, add: !task.assignees.includes(choice.id) } } : { milestone: { id: choice.id } })} />)}{options?.next_page_token && <Action title="More choices" run={async () => { const next = await host.transport.unary('TasksService/ReadOptions', { project, kind: optionKind, page_token: options.next_page_token }); setOptions({ ...next, choices: [...options.choices, ...next.choices] }) }} />}
      <Action title="Clear milestone" run={() => mutate({ milestone: {} })} />
      <Action title="Comments" run={() => readComments()} />{comments?.comments.map(value => <React.Fragment key={value.id}><ThemedText>{value.author} · {value.created_at}</ThemedText><ThemedText selectable>{value.body}</ThemedText>{value.can_edit && <Action title="Edit comment" run={() => { setCommentId(value.id); setComment(value.body) }} />}{value.can_delete && <Action title="Delete comment" run={async () => { await mutate({ comment: { comment_id: value.id, body: '', delete: true } }); await readComments() }} />}</React.Fragment>)}{comments?.next_page_token && <Action title="More comments" run={() => readComments(true)} />}
      <Field label={commentId ? 'Edit comment' : 'New comment'} multiline value={comment} onChangeText={setComment} /><Action title="Save comment" disabled={!comment} run={async () => { await mutate({ comment: { comment_id: commentId, body: comment, delete: false } }); setComment(''); setCommentId(undefined); await readComments() }} />
      {task.attachments.map(artifact => <ArtifactPreview key={artifact.artifact_id} host={host} artifact={artifact} />)}
      {task.task_attachments.map(attachment => <React.Fragment key={attachment.id}><TaskAttachmentPreview host={host} task={task.task!} attachment={attachment} />{attachment.can_delete && <Action title={`Delete ${attachment.name}`} run={() => mutate({ attachment: { delete_id: attachment.id, name: attachment.name } })} />}</React.Fragment>)}
      <Action title="Attach file to task" run={async () => { const source = await host.transport.native.pickFile(); const journal = await prepareUpload(host, { task: task.task! }, 'task-attachment', source.id, source.size, fromBase64(source.sha256), source.mediaType); const artifact = await upload(host, journal.id); await mutate({ attachment: { attach: artifact, name: source.id } }) }} />
      {provider === 'jira' && <><Action title="Choose board" run={() => loadSchema(6)} />{purpose === 6 && schema?.choices.map(choice => <Action key={choice.id} title={choice.label} run={async () => { setBoardId(choice.id); await loadSchema(7, undefined, choice.id) }} />)}{purpose === 7 && schema?.choices.map(choice => <Action key={choice.id} title={`Move to ${choice.label}`} run={() => mutate({ sprint: { id: choice.id } })} />)}{!!boardId && <Action title="Move to backlog" run={() => mutate({ sprint: {} })} />}</>}
      <Field label="Worktree branch" value={branch} onChangeText={setBranch} /><Action title="Create worktree from task" disabled={!branch || !workspace} run={async () => checkReceipt(await host.mutate('TasksService/TaskToWorktree', { command: command(host.store), task: task.task, workspace, branch }))} />
      {provider !== 'github' && <><Field label="Related task" value={otherTask} onChangeText={setOtherTask} /><Action title="Load link types" run={() => loadSchema(5)} />{purpose === 5 && schema?.choices.map(choice => <Action key={choice.id} title={`${linkKind === choice.id ? '✓ ' : ''}${choice.label}`} run={() => setLinkKind(choice.id)} />)}<Action title="Link task" disabled={!otherTask || !linkKind} run={() => mutate({ link: { kind: linkKind, other_task: otherTask, outward: true } })} />
      <Field label="Worklog seconds" keyboardType="number-pad" value={seconds} onChangeText={setSeconds} /><Field label="Worklog started (ISO timestamp)" value={started} onChangeText={setStarted} /><Action title="Add worklog" run={() => { if (!/^\d+$/.test(seconds) || !Number.isFinite(Date.parse(started))) throw Error('Enter duration and timestamp'); return mutate({ worklog: { seconds, started, comment } }) }} />
      {[true, false].map(value => <React.Fragment key={String(value)}><Action title={value ? 'Watch task' : 'Stop watching'} run={() => mutate({ watching: value })} /><Action title={value ? 'Vote' : 'Remove vote'} run={() => mutate({ voted: value })} /></React.Fragment>)}
      <Field label="Type task ID to delete" value={confirmation} onChangeText={setConfirmation} /><Action title="Delete task" disabled={confirmation !== task.task?.task_id} run={() => Alert.alert('Delete task?', 'This deletes the task at its provider.', [{ text: 'Cancel' }, { text: 'Delete', style: 'destructive', onPress: () => { void mutate({ delete: { confirmation, subtasks: false } }).then(() => { setTask(undefined); return search() }).catch(e => setFailure(String(e))) } }])} />
      </>}
    </>}
  </>}</Page>
}
