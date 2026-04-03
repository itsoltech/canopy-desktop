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
  TrackerSprint,
  TrackerStatus,
} from '../types'

interface JiraTaskFields {
  summary?: string
  description?: string
  status?: { name?: string }
  priority?: { name?: string }
  issuetype?: { name?: string; subtask?: boolean }
  parent?: { key?: string }
  assignee?: { displayName?: string; accountId?: string }
  sprint?: { id?: number; name?: string; state?: string }
}

interface JiraTask {
  key: string
  fields: JiraTaskFields
  self?: string
}

function buildAuthHeaders(connection: TaskTrackerConnection, token: string): HeadersInit {
  if (connection.username) {
    const encoded = Buffer.from(`${connection.username}:${token}`).toString('base64')
    return {
      Authorization: `Basic ${encoded}`,
      Accept: 'application/json',
      'Content-Type': 'application/json',
    }
  }
  return {
    Authorization: `Bearer ${token}`,
    Accept: 'application/json',
    'Content-Type': 'application/json',
  }
}

function apiError(status: number, message: string): TaskTrackerError {
  return { _tag: 'ProviderApiError', status, message, provider: 'jira' }
}

function jiraFetch<T>(
  connection: TaskTrackerConnection,
  token: string,
  path: string,
): ResultAsync<T, TaskTrackerError> {
  const url = `${connection.baseUrl.replace(/\/$/, '')}${path}`
  return fromExternalCall(
    fetch(url, {
      headers: buildAuthHeaders(connection, token),
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

function mapTaskType(fields: JiraTaskFields): string {
  const name = fields.issuetype?.name?.toLowerCase() ?? ''
  if (fields.issuetype?.subtask) return 'subtask'
  if (name.includes('story') || name.includes('user story')) return 'story'
  if (name.includes('bug')) return 'bug'
  if (name.includes('epic')) return 'epic'
  return name || 'task'
}

function parseSprintNumber(name: string): number | undefined {
  const match = name.match(/\d+/)
  return match ? parseInt(match[0], 10) : undefined
}

function adfToPlainText(node: unknown): string {
  if (!node || typeof node !== 'object') return typeof node === 'string' ? node : ''
  const n = node as { type?: string; text?: string; content?: unknown[] }
  if (n.type === 'text') return n.text ?? ''
  if (Array.isArray(n.content)) {
    const parts = n.content.map(adfToPlainText)
    if (n.type === 'paragraph' || n.type === 'heading') return parts.join('') + '\n'
    if (n.type === 'listItem') return '• ' + parts.join('')
    return parts.join('')
  }
  return ''
}

function mapJiraTask(task: JiraTask, baseUrl: string): TrackerTask {
  const f = task.fields
  return {
    key: task.key,
    summary: f.summary ?? '',
    description:
      typeof f.description === 'string' ? f.description : adfToPlainText(f.description).trim(),
    status: f.status?.name ?? '',
    priority: f.priority?.name ?? '',
    type: mapTaskType(f),
    parentKey: f.parent?.key,
    sprintName: f.sprint?.name,
    sprintNumber: f.sprint?.name ? parseSprintNumber(f.sprint.name) : undefined,
    assignee: f.assignee?.displayName,
    url: `${baseUrl.replace(/\/$/, '')}/browse/${task.key}`,
  }
}

export const jiraClient: TaskTrackerProviderClient = {
  testConnection(connection, token) {
    return jiraFetch(connection, token, '/rest/api/3/myself').map(() => true)
  },

  getCurrentUserDisplayName(connection, token) {
    return jiraFetch<{ displayName?: string }>(connection, token, '/rest/api/3/myself').map(
      (data) => data.displayName ?? '',
    )
  },

  fetchTaskByKey(connection, token, taskKey) {
    const fields = 'summary,description,status,priority,issuetype,parent,assignee,sprint'
    return jiraFetch<JiraTask>(
      connection,
      token,
      `/rest/api/3/issue/${encodeURIComponent(taskKey)}?fields=${fields}`,
    ).map((data) => mapJiraTask(data, connection.baseUrl) as TrackerTask | null)
  },

  fetchBoards(connection, token) {
    const params = connection.projectKey
      ? `?projectKeyOrId=${encodeURIComponent(connection.projectKey)}`
      : '?maxResults=50'
    return jiraFetch<{
      values: Array<{
        id: number
        name: string
        location?: { projectKey?: string }
      }>
    }>(connection, token, `/rest/agile/1.0/board${params}`).map((data) =>
      data.values.map(
        (b): TrackerBoard => ({
          id: String(b.id),
          name: b.name,
          projectKey: b.location?.projectKey,
        }),
      ),
    )
  },

  fetchStatuses(connection, token) {
    return jiraFetch<Array<{ id: string; name: string; statusCategory?: { key?: string } }>>(
      connection,
      token,
      '/rest/api/3/statuses',
    )
      .map((data) => {
        const seen = new Set<string>()
        const statuses: TrackerStatus[] = []
        for (const s of data) {
          if (!seen.has(s.name)) {
            seen.add(s.name)
            statuses.push({ id: s.id, name: s.name })
          }
        }
        return statuses
      })
      .orElse(() => {
        if (connection.projectKey) {
          return jiraFetch<Array<{ statuses?: Array<{ id: string; name: string }> }>>(
            connection,
            token,
            `/rest/api/3/project/${encodeURIComponent(connection.projectKey)}/statuses`,
          ).map((data) => {
            const seen = new Set<string>()
            const statuses: TrackerStatus[] = []
            for (const category of data) {
              for (const s of category.statuses ?? []) {
                if (!seen.has(s.name)) {
                  seen.add(s.name)
                  statuses.push({ id: s.id, name: s.name })
                }
              }
            }
            return statuses
          })
        }
        return errAsync(apiError(0, 'No statuses available'))
      })
  },

  fetchTasks(connection, token, params) {
    const resolvedBoardId = params.boardId || connection.boardId
    const fields = 'summary,status,priority,issuetype,parent,assignee,sprint'

    if (resolvedBoardId) {
      const jql = 'statusCategory != Done ORDER BY updated DESC'
      const jqlParam = `&jql=${encodeURIComponent(jql)}`

      return jiraFetch<{ issues: JiraTask[] }>(
        connection,
        token,
        `/rest/agile/1.0/board/${encodeURIComponent(resolvedBoardId)}/issue?fields=${fields}&maxResults=200${jqlParam}`,
      ).map((data) => data.issues.map((i) => mapJiraTask(i, connection.baseUrl)))
    }

    const jqlParts: string[] = []
    if (connection.projectKey && /^[A-Za-z0-9_-]+$/.test(connection.projectKey)) {
      jqlParts.push(`project = "${connection.projectKey}"`)
    }
    jqlParts.push('assignee = currentUser()')

    const jql = jqlParts.join(' AND ') + ' ORDER BY updated DESC'
    return jiraFetch<{ issues: JiraTask[] }>(
      connection,
      token,
      `/rest/api/3/search/jql?jql=${encodeURIComponent(jql)}&fields=${encodeURIComponent(fields)}&maxResults=200`,
    ).map((data) => data.issues.map((i) => mapJiraTask(i, connection.baseUrl)))
  },

  getCurrentSprint(connection, token, boardId) {
    const getBoardId: ResultAsync<string, TaskTrackerError> = boardId
      ? okAsync(boardId)
      : jiraClient.fetchBoards(connection, token).andThen((boards) => {
          if (boards.length === 0) return errAsync(apiError(0, 'No boards found'))
          return okAsync(boards[0].id)
        })

    return getBoardId.andThen((resolvedBoardId) =>
      jiraFetch<{
        values: Array<{ id: number; name: string; state: string }>
      }>(
        connection,
        token,
        `/rest/agile/1.0/board/${encodeURIComponent(resolvedBoardId)}/sprint?state=active&maxResults=1`,
      ).map((data) => {
        const sprint = data.values[0]
        if (!sprint) return null

        return {
          id: String(sprint.id),
          name: sprint.name,
          number: parseSprintNumber(sprint.name),
          state: sprint.state as TrackerSprint['state'],
        }
      }),
    )
  },

  fetchTaskComments(connection, token, taskKey) {
    return jiraFetch<{
      comments: Array<{
        id: string
        body?: unknown
        author?: { displayName?: string }
        created?: string
      }>
    }>(
      connection,
      token,
      `/rest/api/3/issue/${encodeURIComponent(taskKey)}/comment?maxResults=50`,
    ).map((data) =>
      (data.comments ?? []).map(
        (c): TrackerComment => ({
          id: c.id,
          author: c.author?.displayName ?? '',
          body: typeof c.body === 'string' ? c.body : adfToPlainText(c.body).trim(),
          created: c.created ?? '',
        }),
      ),
    )
  },

  fetchTaskAttachments(connection, token, taskKey) {
    return jiraFetch<{
      fields: {
        attachment?: Array<{
          id: string
          filename?: string
          mimeType?: string
          size?: number
          content?: string
        }>
      }
    }>(connection, token, `/rest/api/3/issue/${encodeURIComponent(taskKey)}?fields=attachment`).map(
      (data) =>
        (data.fields.attachment ?? []).map(
          (a): TrackerAttachment => ({
            id: a.id,
            name: a.filename ?? '',
            mimeType: a.mimeType ?? '',
            size: a.size ?? 0,
            url: a.content ?? '',
          }),
        ),
    )
  },
}
