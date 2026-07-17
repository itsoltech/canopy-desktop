import { okAsync, errAsync, type ResultAsync } from 'neverthrow'
import { match } from 'ts-pattern'
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
  TrackerStatusCategory,
  TrackerTransition,
  TrackerTransitionField,
} from '../types'

interface JiraTaskFields {
  summary?: string
  description?: string
  status?: { name?: string; statusCategory?: { key?: string } }
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
      // Do not follow redirects: baseUrl comes from repo config, and a redirect
      // would forward the Authorization token to an attacker-controlled host.
      redirect: 'error',
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

/** POST to Jira and ignore the response body (transitions return 204, comment returns 201). */
function jiraSend(
  connection: TaskTrackerConnection,
  token: string,
  path: string,
  body: unknown,
): ResultAsync<void, TaskTrackerError> {
  const url = `${connection.baseUrl.replace(/\/$/, '')}${path}`
  return fromExternalCall(
    fetch(url, {
      method: 'POST',
      headers: buildAuthHeaders(connection, token),
      body: JSON.stringify(body),
      // Do not follow redirects: baseUrl comes from repo config, and a redirect
      // would forward the Authorization token to an attacker-controlled host.
      redirect: 'error',
      signal: AbortSignal.timeout(15_000),
    }),
    (e) => apiError(0, errorMessage(e)),
  ).andThen((res) => {
    if (!res.ok) {
      return fromExternalCall(
        res.text().catch(() => ''),
        (e) => apiError(res.status, errorMessage(e)),
      ).andThen((text) => errAsync(apiError(res.status, text || res.statusText)))
    }
    return okAsync(undefined)
  })
}

/** Jira Cloud v3 write endpoints take comment bodies in Atlassian Document Format. */
function toAdf(text: string): unknown {
  return {
    type: 'doc',
    version: 1,
    content: text.split('\n').map((line) => ({
      type: 'paragraph',
      content: line ? [{ type: 'text', text: line }] : [],
    })),
  }
}

// Jira's three fixed status categories; every status belongs to exactly one.
const STATUS_CATEGORY_MAP: Record<string, TrackerStatusCategory> = {
  new: 'todo',
  indeterminate: 'in-progress',
  done: 'done',
}

interface JiraTransitionsResponse {
  transitions?: Array<{
    id: string
    name?: string
    to?: { name?: string; statusCategory?: { key?: string } }
    fields?: Record<
      string,
      {
        name?: string
        required?: boolean
        allowedValues?: Array<{ id?: string | number; name?: string; value?: string }>
      }
    >
  }>
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

function normalizeTrackerText(text: string): string {
  return text
    .replace(/\r\n?/g, '\n')
    .replace(/\n{3,}/g, '\n\n')
    .trim()
}

function adfToPlainText(node: unknown, depth = 0): string {
  if (!node || typeof node !== 'object') return typeof node === 'string' ? node : ''
  // Bail out past a sane nesting depth so a pathologically deep ADF document
  // (untrusted API response) cannot blow the main-process call stack.
  if (depth > 100) return ''
  const n = node as { type?: string; text?: string; content?: unknown[] }
  return match(n)
    .with({ type: 'text' }, (x) => x.text ?? '')
    .with({ type: 'hardBreak' }, () => '\n')
    .when(
      (x) => Array.isArray(x.content),
      (x) => {
        const parts = (x.content as unknown[]).map((c) => adfToPlainText(c, depth + 1))
        return match(x.type)
          .with('paragraph', 'heading', () => parts.join('').trimEnd() + '\n\n')
          .with('listItem', () => '• ' + parts.join('').trim() + '\n')
          .with('bulletList', 'orderedList', () => parts.join('') + '\n')
          .otherwise(() => parts.join(''))
      },
    )
    .otherwise(() => '')
}

function mapJiraTask(task: JiraTask, baseUrl: string): TrackerTask {
  const f = task.fields
  return {
    key: task.key,
    summary: f.summary ?? '',
    description:
      typeof f.description === 'string'
        ? normalizeTrackerText(f.description)
        : normalizeTrackerText(adfToPlainText(f.description)),
    status: f.status?.name ?? '',
    statusCategory: STATUS_CATEGORY_MAP[f.status?.statusCategory?.key ?? ''],
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
      data.values.map((b): TrackerBoard => ({
        id: String(b.id),
        name: b.name,
        projectKey: b.location?.projectKey,
      })),
    )
  },

  fetchProjects(connection, token) {
    return jiraFetch<{ values: Array<{ key: string; name: string }> }>(
      connection,
      token,
      '/rest/api/3/project/search?maxResults=100',
    ).map((data) => data.values.map((p) => ({ key: p.key, name: p.name })))
  },

  fetchTaskTypes(connection, token) {
    // Global issue-type list; names are deduped (Jira repeats them per project scope).
    return jiraFetch<Array<{ name: string }>>(connection, token, '/rest/api/3/issuetype').map(
      (data) => [...new Set(data.map((t) => t.name).filter(Boolean))],
    )
  },

  fetchStatuses(connection, token) {
    // `/rest/api/3/status` (singular) lists ALL statuses with their category; the plural
    // `/statuses` endpoint requires explicit status ids and 400s without them.
    return jiraFetch<Array<{ id: string; name: string; statusCategory?: { key?: string } }>>(
      connection,
      token,
      '/rest/api/3/status',
    )
      .map((data) => {
        const seen = new Set<string>()
        const statuses: TrackerStatus[] = []
        for (const s of data) {
          if (!seen.has(s.name)) {
            seen.add(s.name)
            statuses.push({
              id: s.id,
              name: s.name,
              statusCategory: STATUS_CATEGORY_MAP[s.statusCategory?.key ?? ''],
            })
          }
        }
        return statuses
      })
      .orElse(() => {
        if (connection.projectKey) {
          return jiraFetch<
            Array<{
              statuses?: Array<{ id: string; name: string; statusCategory?: { key?: string } }>
            }>
          >(
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
                  statuses.push({
                    id: s.id,
                    name: s.name,
                    statusCategory: STATUS_CATEGORY_MAP[s.statusCategory?.key ?? ''],
                  })
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
      (data.comments ?? []).map((c): TrackerComment => ({
        id: c.id,
        author: c.author?.displayName ?? '',
        body:
          typeof c.body === 'string'
            ? normalizeTrackerText(c.body)
            : normalizeTrackerText(adfToPlainText(c.body)),
        created: c.created ?? '',
      })),
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
        (data.fields.attachment ?? []).map((a): TrackerAttachment => ({
          id: a.id,
          name: a.filename ?? '',
          mimeType: a.mimeType ?? '',
          size: a.size ?? 0,
          url: a.content ?? '',
        })),
    )
  },

  fetchTransitions(connection, token, taskKey) {
    // expand=transitions.fields returns each transition's screen fields with `required` flags and
    // allowed values (e.g. resolution: Done / Won't Do) — the workflow requirements, introspected.
    return jiraFetch<JiraTransitionsResponse>(
      connection,
      token,
      `/rest/api/3/issue/${encodeURIComponent(taskKey)}/transitions?expand=transitions.fields`,
    ).map((data) =>
      (data.transitions ?? []).map((t): TrackerTransition => ({
        id: t.id,
        name: t.name ?? t.to?.name ?? t.id,
        toStatus: t.to?.name ?? '',
        toStatusCategory: STATUS_CATEGORY_MAP[t.to?.statusCategory?.key ?? ''],
        fields: Object.entries(t.fields ?? {}).map(([key, f]): TrackerTransitionField => ({
          key,
          name: f.name ?? key,
          required: f.required ?? false,
          allowedValues: f.allowedValues?.map((v) => ({
            id: String(v.id ?? v.value ?? ''),
            name: v.name ?? v.value ?? String(v.id ?? ''),
          })),
        })),
      })),
    )
  },

  applyTransition(connection, token, taskKey, transitionId, opts) {
    const fieldEntries = Object.entries(opts.fields ?? {}).filter(([, v]) => v)
    const body: Record<string, unknown> = { transition: { id: transitionId } }
    if (fieldEntries.length > 0) {
      // Screen fields are sent by id (resolution and friends are option fields keyed by id).
      body.fields = Object.fromEntries(fieldEntries.map(([key, id]) => [key, { id }]))
    }
    if (opts.comment) {
      body.update = { comment: [{ add: { body: toAdf(opts.comment) } }] }
    }
    return jiraSend(
      connection,
      token,
      `/rest/api/3/issue/${encodeURIComponent(taskKey)}/transitions`,
      body,
    )
  },

  addComment(connection, token, taskKey, commentBody) {
    return jiraSend(connection, token, `/rest/api/3/issue/${encodeURIComponent(taskKey)}/comment`, {
      body: toAdf(commentBody),
    })
  },
}
