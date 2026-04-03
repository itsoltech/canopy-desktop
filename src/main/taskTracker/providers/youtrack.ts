import { okAsync, errAsync, type ResultAsync } from 'neverthrow'
import type { TaskTrackerError } from '../errors'
import { fromExternalCall, errorMessage } from '../../errors'
import type {
  TaskTrackerConnection,
  TaskTrackerProviderClient,
  TrackerAttachment,
  TrackerBoard,
  TrackerComment,
  TrackerTask,
  TrackerStatus,
} from '../types'

interface YTTask {
  id: string
  idReadable: string
  summary?: string
  description?: string
  fields?: Array<{
    projectCustomField?: { field?: { name?: string } }
    name?: string
    value?:
      | { name?: string; login?: string }
      | Array<{ name?: string }>
      | { name?: string; id?: string }
  }>
  parent?: { issues?: Array<{ idReadable?: string }> }
}

function buildHeaders(token: string): HeadersInit {
  return {
    Authorization: `Bearer ${token}`,
    Accept: 'application/json',
    'Content-Type': 'application/json',
  }
}

function apiError(status: number, message: string): TaskTrackerError {
  return { _tag: 'ProviderApiError', status, message, provider: 'youtrack' }
}

function ytFetch<T>(
  connection: TaskTrackerConnection,
  token: string,
  path: string,
): ResultAsync<T, TaskTrackerError> {
  const url = `${connection.baseUrl.replace(/\/$/, '')}${path}`
  return fromExternalCall(
    fetch(url, {
      headers: buildHeaders(token),
      signal: AbortSignal.timeout(15_000),
    }),
    (e) => apiError(0, errorMessage(e)),
  ).andThen((res) => {
    if (!res.ok) {
      return fromExternalCall(
        res.text().catch(() => ''),
        (e) => apiError(res.status, errorMessage(e)),
      ).andThen((body) => errAsync(apiError(res.status, body || res.statusText)))
    }
    return fromExternalCall(res.json() as Promise<T>, (e) => apiError(0, errorMessage(e)))
  })
}

function extractField(task: YTTask, fieldName: string): string {
  const field = task.fields?.find(
    (f) => f.name === fieldName || f.projectCustomField?.field?.name === fieldName,
  )
  if (!field?.value) return ''
  if (Array.isArray(field.value)) return field.value[0]?.name ?? ''
  return field.value.name ?? ''
}

function mapTaskType(typeStr: string): string {
  const lower = typeStr.toLowerCase()
  if (lower.includes('story') || lower.includes('user story')) return 'story'
  if (lower.includes('bug')) return 'bug'
  if (lower.includes('epic')) return 'epic'
  if (lower.includes('sub') || lower.includes('subtask')) return 'subtask'
  return lower || 'task'
}

function parseSprintNumber(name: string): number | undefined {
  const match = name.match(/\d+/)
  return match ? parseInt(match[0], 10) : undefined
}

function mapYTTask(task: YTTask, baseUrl: string): TrackerTask {
  const status = extractField(task, 'State')
  const priority = extractField(task, 'Priority')
  const typeStr = extractField(task, 'Type')
  const assignee = extractField(task, 'Assignee')
  const sprintName = extractField(task, 'Sprints') || extractField(task, 'Sprint')
  const parentKey = task.parent?.issues?.[0]?.idReadable

  return {
    key: task.idReadable,
    summary: task.summary ?? '',
    description: task.description ?? '',
    status,
    priority,
    type: mapTaskType(typeStr),
    parentKey,
    sprintName: sprintName || undefined,
    sprintNumber: sprintName ? parseSprintNumber(sprintName) : undefined,
    assignee: assignee || undefined,
    url: `${baseUrl.replace(/\/$/, '')}/issue/${task.idReadable}`,
  }
}

export const youtrackClient: TaskTrackerProviderClient = {
  testConnection(connection, token) {
    return ytFetch(connection, token, '/api/users/me?fields=id,login').map(() => true)
  },

  getCurrentUserDisplayName(connection, token) {
    return ytFetch<{ name?: string; fullName?: string }>(
      connection,
      token,
      '/api/users/me?fields=name,fullName',
    ).map((data) => data.fullName ?? data.name ?? '')
  },

  fetchTaskByKey(connection, token, taskKey) {
    const fields =
      'id,idReadable,summary,description,fields(name,projectCustomField(field(name)),value(name,login)),parent(issues(idReadable))'
    return ytFetch<YTTask>(
      connection,
      token,
      `/api/issues/${encodeURIComponent(taskKey)}?fields=${encodeURIComponent(fields)}`,
    ).map((data) => mapYTTask(data, connection.baseUrl) as TrackerTask | null)
  },

  fetchBoards(connection, token) {
    return ytFetch<
      Array<{
        id: string
        name: string
        projects?: Array<{ shortName?: string }>
      }>
    >(connection, token, `/api/agiles?fields=id,name,projects(shortName)&$top=50`).map((data) =>
      data.map(
        (b): TrackerBoard => ({
          id: b.id,
          name: b.name,
          projectKey: b.projects?.[0]?.shortName,
        }),
      ),
    )
  },

  fetchStatuses(connection, token) {
    const projectKey = connection.projectKey
    if (!projectKey) return okAsync([])

    return ytFetch<
      Array<{
        id: string
        name: string
        values?: Array<{ name: string }>
      }>
    >(
      connection,
      token,
      `/api/admin/projects/${encodeURIComponent(projectKey)}/customFields?fields=id,name,bundle(values(name))&$top=50`,
    ).map((data) => {
      const stateField = data.find(
        (f) => f.name === 'State' || f.name.toLowerCase().includes('state'),
      )
      if (!stateField?.values) return []

      return stateField.values.map(
        (v): TrackerStatus => ({
          id: v.name,
          name: v.name,
        }),
      )
    })
  },

  fetchTasks(connection, token, params) {
    const projectFromBoard = params.boardId
      ? ytFetch<{ projects?: Array<{ shortName?: string }> }>(
          connection,
          token,
          `/api/agiles/${encodeURIComponent(params.boardId)}?fields=projects(shortName)`,
        )
          .map((board) => board.projects?.[0]?.shortName ?? '')
          .unwrapOr('')
      : Promise.resolve('')

    const resolvedProject = connection.projectKey
      ? Promise.resolve(connection.projectKey)
      : projectFromBoard

    return fromExternalCall(resolvedProject, (e) => apiError(0, errorMessage(e))).andThen(
      (projectKey) => {
        const queryParts: string[] = []

        if (projectKey && /^[A-Za-z0-9_-]+$/.test(projectKey)) {
          queryParts.push(`project: {${projectKey}}`)
        }

        if (params.assignedToMe) {
          queryParts.push('for: me')
        }

        const query = queryParts.join(' ') + ' sort by: updated desc'
        const fields =
          'id,idReadable,summary,fields(name,projectCustomField(field(name)),value(name,login)),parent(issues(idReadable))'
        return ytFetch<YTTask[]>(
          connection,
          token,
          `/api/issues?query=${encodeURIComponent(query)}&fields=${encodeURIComponent(fields)}&$top=200`,
        ).map((data) => data.map((i) => mapYTTask(i, connection.baseUrl)))
      },
    )
  },

  getCurrentSprint(connection, token, boardId) {
    const getBoardId = boardId
      ? okAsync<string, TaskTrackerError>(boardId)
      : youtrackClient.fetchBoards(connection, token).andThen((boards) => {
          if (boards.length === 0) return errAsync(apiError(0, 'No boards found'))
          return okAsync<string, TaskTrackerError>(boards[0].id)
        })

    return getBoardId.andThen((resolvedBoardId) =>
      ytFetch<
        Array<{ id: string; name: string; isResolved?: boolean; start?: number; finish?: number }>
      >(
        connection,
        token,
        `/api/agiles/${encodeURIComponent(resolvedBoardId)}/sprints?fields=id,name,isResolved,start,finish&$top=10`,
      ).map((data) => {
        const now = Date.now()
        const active = data.find(
          (s) => !s.isResolved && s.start && s.finish && s.start <= now && s.finish >= now,
        )
        if (!active) {
          const unresolved = data.find((s) => !s.isResolved)
          if (!unresolved) return null
          return {
            id: unresolved.id,
            name: unresolved.name,
            number: parseSprintNumber(unresolved.name),
            state: 'active' as const,
          }
        }

        return {
          id: active.id,
          name: active.name,
          number: parseSprintNumber(active.name),
          state: 'active' as const,
        }
      }),
    )
  },

  fetchTaskComments(connection, token, taskKey) {
    const fields = 'id,text,author(name,fullName),created'
    return ytFetch<
      Array<{
        id: string
        text?: string
        author?: { name?: string; fullName?: string }
        created?: number
      }>
    >(
      connection,
      token,
      `/api/issues/${encodeURIComponent(taskKey)}/comments?fields=${encodeURIComponent(fields)}`,
    ).map((data) =>
      data.map(
        (c): TrackerComment => ({
          id: c.id,
          author: c.author?.fullName ?? c.author?.name ?? '',
          body: c.text ?? '',
          created: c.created ? new Date(c.created).toISOString() : '',
        }),
      ),
    )
  },

  fetchTaskAttachments(connection, token, taskKey) {
    const fields = 'attachments(id,name,size,mimeType)'
    return ytFetch<{
      attachments?: Array<{
        id: string
        name?: string
        size?: number
        mimeType?: string
      }>
    }>(
      connection,
      token,
      `/api/issues/${encodeURIComponent(taskKey)}?fields=${encodeURIComponent(fields)}`,
    ).map((data) => {
      const baseUrl = connection.baseUrl.replace(/\/$/, '')
      return (data.attachments ?? []).map(
        (a): TrackerAttachment => ({
          id: a.id,
          name: a.name ?? '',
          mimeType: a.mimeType ?? '',
          size: a.size ?? 0,
          url: `${baseUrl}/api/issues/${encodeURIComponent(taskKey)}/attachments/${a.id}/file`,
        }),
      )
    })
  },
}
