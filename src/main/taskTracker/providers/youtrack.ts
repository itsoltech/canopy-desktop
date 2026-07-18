import { okAsync, errAsync, type ResultAsync } from 'neverthrow'
import { taskTrackerErrorMessage, type TaskTrackerError } from '../errors'
import { fromExternalCall, errorMessage } from '../../errors'
import type {
  TaskTrackerConnection,
  TaskTrackerProviderClient,
  TrackerAttachment,
  TrackerBoard,
  TrackerComment,
  TrackerTask,
  TrackerStatus,
  TrackerTransition,
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
      { name?: string; login?: string } | Array<{ name?: string }> | { name?: string; id?: string }
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

/** POST to YouTrack and parse the JSON response (issue creation returns the new id in the body). */
function ytPost<T>(
  connection: TaskTrackerConnection,
  token: string,
  path: string,
  body: unknown,
): ResultAsync<T, TaskTrackerError> {
  const url = `${connection.baseUrl.replace(/\/$/, '')}${path}`
  return fromExternalCall(
    fetch(url, {
      method: 'POST',
      headers: buildHeaders(token),
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
    return fromExternalCall(res.json() as Promise<T>, (e) => apiError(0, errorMessage(e)))
  })
}

/** The issues API takes the INTERNAL project id, while Canopy works with the shortName prefix. */
function resolveProjectId(
  connection: TaskTrackerConnection,
  token: string,
  projectKey: string,
): ResultAsync<string, TaskTrackerError> {
  if (!projectKey) return errAsync(apiError(400, 'A project is required to create a task'))
  return ytFetch<Array<{ id?: string; shortName?: string }>>(
    connection,
    token,
    '/api/admin/projects?fields=id,shortName&$top=100',
  ).andThen((projects) => {
    const found = projects.find(
      (p) => p.shortName?.toUpperCase() === projectKey.toUpperCase() && p.id,
    )
    if (!found) return errAsync(apiError(404, `Project not found: ${projectKey}`))
    return okAsync(found.id as string)
  })
}

/** POST to YouTrack and ignore the response body. */
function ytSend(
  connection: TaskTrackerConnection,
  token: string,
  path: string,
  body: unknown,
): ResultAsync<void, TaskTrackerError> {
  const url = `${connection.baseUrl.replace(/\/$/, '')}${path}`
  return fromExternalCall(
    fetch(url, {
      method: 'POST',
      headers: buildHeaders(token),
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

/**
 * State bundle values the tracker offers for this task (minus the current one). YouTrack has no
 * transition introspection — workflow rules live in scripts and only surface as errors on apply —
 * so these plain state names are both the "transitions" we present and the whitelist we validate
 * an apply request against.
 */
function fetchAvailableStates(
  connection: TaskTrackerConnection,
  token: string,
  taskKey: string,
): ResultAsync<Array<{ name: string; resolved: boolean }>, TaskTrackerError> {
  const fields =
    'customFields(name,value(name),projectCustomField(field(name),bundle(values(name,archived,isResolved))))'
  return ytFetch<{
    customFields?: Array<{
      name?: string
      value?: { name?: string }
      projectCustomField?: {
        field?: { name?: string }
        bundle?: {
          values?: Array<{ name?: string; archived?: boolean; isResolved?: boolean }>
        }
      }
    }>
  }>(
    connection,
    token,
    `/api/issues/${encodeURIComponent(taskKey)}?fields=${encodeURIComponent(fields)}`,
  ).map((data) => {
    const stateField = (data.customFields ?? []).find(
      (f) => (f.projectCustomField?.field?.name ?? f.name) === 'State',
    )
    const current = stateField?.value?.name ?? ''
    const values = stateField?.projectCustomField?.bundle?.values ?? []
    return values
      .filter((v) => v.name && !v.archived && v.name !== current)
      .map((v) => ({ name: v.name!, resolved: v.isResolved ?? false }))
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
      data.map((b): TrackerBoard => ({
        id: b.id,
        name: b.name,
        projectKey: b.projects?.[0]?.shortName,
      })),
    )
  },

  fetchProjects(connection, token) {
    // shortName is the issue-id prefix (PROJ in PROJ-123) — the project key in Canopy terms.
    return ytFetch<Array<{ shortName?: string; name?: string }>>(
      connection,
      token,
      '/api/admin/projects?fields=shortName,name&$top=100',
    ).map((data) =>
      data
        .filter((p) => p.shortName)
        .map((p) => ({ key: p.shortName as string, name: p.name || (p.shortName as string) })),
    )
  },

  fetchTaskTypes(connection, token) {
    const projectKey = connection.projectKey
    if (!projectKey) return okAsync([])
    return ytFetch<Array<{ name: string; values?: Array<{ name: string }> }>>(
      connection,
      token,
      `/api/admin/projects/${encodeURIComponent(projectKey)}/customFields?fields=name,bundle(values(name))&$top=50`,
    ).map((data) => {
      const typeField = data.find((f) => f.name === 'Type' || f.name.toLowerCase() === 'type')
      return (typeField?.values ?? []).map((v) => v.name).filter(Boolean)
    })
  },

  fetchStatuses(connection, token) {
    const projectKey = connection.projectKey
    if (!projectKey) return okAsync([])

    return ytFetch<
      Array<{
        id: string
        name: string
        values?: Array<{ name: string; isResolved?: boolean }>
      }>
    >(
      connection,
      token,
      `/api/admin/projects/${encodeURIComponent(projectKey)}/customFields?fields=id,name,bundle(values(name,isResolved))&$top=50`,
    ).map((data) => {
      const stateField = data.find(
        (f) => f.name === 'State' || f.name.toLowerCase().includes('state'),
      )
      if (!stateField?.values) return []

      return stateField.values.map((v): TrackerStatus => ({
        id: v.name,
        name: v.name,
        statusCategory: v.isResolved ? 'done' : undefined,
      }))
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

    const resolvedProject = params.projectKey
      ? Promise.resolve(params.projectKey)
      : connection.projectKey
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
      data.map((c): TrackerComment => ({
        id: c.id,
        author: c.author?.fullName ?? c.author?.name ?? '',
        body: c.text ?? '',
        created: c.created ? new Date(c.created).toISOString() : '',
      })),
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
      return (data.attachments ?? []).map((a): TrackerAttachment => ({
        id: a.id,
        name: a.name ?? '',
        mimeType: a.mimeType ?? '',
        size: a.size ?? 0,
        url: `${baseUrl}/api/issues/${encodeURIComponent(taskKey)}/attachments/${a.id}/file`,
      }))
    })
  },

  fetchTransitions(connection, token, taskKey) {
    return fetchAvailableStates(connection, token, taskKey).map((states) =>
      states.map((s): TrackerTransition => ({
        id: s.name,
        name: s.name,
        toStatus: s.name,
        // YouTrack has no Jira-style categories; resolved states are the closest signal.
        toStatusCategory: s.resolved ? 'done' : undefined,
        fields: [],
      })),
    )
  },

  applyTransition(connection, token, taskKey, transitionId, opts) {
    // `transitionId` crosses the renderer IPC boundary, and the Commands API query is free-text
    // (a crafted string could smuggle extra commands executed with the user's token). Only accept
    // ids that match a state the tracker actually offers for this task right now.
    return fetchAvailableStates(connection, token, taskKey).andThen((states) => {
      const state = states.find((s) => s.name === transitionId)
      if (!state) {
        return errAsync(apiError(400, `Unknown transition for ${taskKey}: ${transitionId}`))
      }
      // Commands API applies the state change and (optionally) a comment atomically; workflow
      // violations come back as a 4xx whose message we surface verbatim.
      const body: Record<string, unknown> = {
        query: `State "${state.name.replace(/"/g, '')}"`,
        issues: [{ idReadable: taskKey }],
      }
      if (opts.comment) body.comment = opts.comment
      return ytSend(connection, token, `/api/commands`, body)
    })
  },

  addComment(connection, token, taskKey, body) {
    return ytSend(connection, token, `/api/issues/${encodeURIComponent(taskKey)}/comments`, {
      text: body,
    })
  },

  fetchAssignableUsers(connection, token, projectKey) {
    // Project team is the natural assignee pool; reading it needs admin scope on some
    // instances, so fall back to the global user list rather than failing the form.
    const base = connection.baseUrl.replace(/\/$/, '')
    return resolveProjectId(connection, token, projectKey)
      .andThen((projectId) =>
        ytFetch<{
          users?: Array<{ login?: string; fullName?: string; name?: string; avatarUrl?: string }>
        }>(
          connection,
          token,
          `/api/admin/projects/${encodeURIComponent(projectId)}/team?fields=users(login,fullName,name,avatarUrl)`,
        ).map((team) => team.users ?? []),
      )
      .orElse(() =>
        ytFetch<Array<{ login?: string; fullName?: string; name?: string; avatarUrl?: string }>>(
          connection,
          token,
          '/api/users?fields=login,fullName,name,avatarUrl&$top=50',
        ),
      )
      .map((users) =>
        users
          .filter((u) => u.login)
          .map((u) => ({
            id: u.login!,
            displayName: u.fullName ?? u.name ?? u.login!,
            // YouTrack returns hub-relative avatar paths — absolutize against the instance.
            avatarUrl: u.avatarUrl
              ? u.avatarUrl.startsWith('http')
                ? u.avatarUrl
                : `${base}${u.avatarUrl}`
              : undefined,
          })),
      )
  },

  fetchSprints(connection, token, boardId) {
    return ytFetch<Array<{ id: string; name: string; isResolved?: boolean; archived?: boolean }>>(
      connection,
      token,
      `/api/agiles/${encodeURIComponent(boardId)}/sprints?fields=id,name,isResolved,archived&$top=50`,
    ).map((data) =>
      data
        .filter((s) => !s.isResolved && !s.archived)
        .map((s) => ({
          id: s.id,
          name: s.name,
          number: parseSprintNumber(s.name),
          state: 'active' as const,
        })),
    )
  },

  fetchCreateTaskTypes(connection, token, projectKey) {
    // Same Type-bundle lookup as fetchTaskTypes, but for an explicit project instead of the
    // connection-level default (the create form picks the project itself). No type icons here —
    // YouTrack bundles don't carry them.
    return ytFetch<Array<{ name: string; values?: Array<{ name: string }> }>>(
      connection,
      token,
      `/api/admin/projects/${encodeURIComponent(projectKey)}/customFields?fields=name,bundle(values(name))&$top=50`,
    ).map((data) => {
      const typeField = data.find((f) => f.name === 'Type' || f.name.toLowerCase() === 'type')
      return (typeField?.values ?? [])
        .map((v) => v.name)
        .filter(Boolean)
        .map((name) => ({ name }))
    })
  },

  createTask(connection, token, input) {
    // Brace-quote a value for the free-text Commands API; braces/quotes are stripped so the
    // value cannot terminate the literal and smuggle extra commands (IPC validates charsets too).
    const q = (v: string): string => `{${v.replace(/[{}"]/g, '')}}`

    return resolveProjectId(connection, token, input.projectKey ?? '')
      .andThen((projectId) =>
        ytPost<{ idReadable?: string }>(connection, token, '/api/issues?fields=idReadable', {
          project: { id: projectId },
          summary: input.title,
          ...(input.description ? { description: input.description } : {}),
        }),
      )
      .andThen((created) => {
        if (!created.idReadable) {
          return errAsync(apiError(0, 'YouTrack did not return the new issue id'))
        }
        const key = created.idReadable
        const warnings: string[] = []
        // From here the issue EXISTS — every follow-up command failure (workflow scripts,
        // permissions) degrades to a warning; a hard error would invite a duplicating retry.
        const cmd = (query: string, label: string): ResultAsync<void, TaskTrackerError> =>
          ytSend(connection, token, '/api/commands', {
            query,
            issues: [{ idReadable: key }],
          }).orElse((e) => {
            warnings.push(
              `Task created, but ${label} was not applied: ${taskTrackerErrorMessage(e)}`,
            )
            return okAsync(undefined)
          })

        let chain: ResultAsync<void, TaskTrackerError> = okAsync(undefined)
        if (input.typeName) {
          chain = chain.andThen(() => cmd(`Type ${q(input.typeName!)}`, 'the type'))
        }
        if (input.assigneeId) {
          chain = chain.andThen(() => cmd(`for ${q(input.assigneeId!)}`, 'the assignee'))
        }
        if (input.sprintId && input.boardId) {
          chain = chain.andThen(() =>
            // The command addresses board and sprint by NAME — resolve both from their ids.
            ytFetch<{ name?: string }>(
              connection,
              token,
              `/api/agiles/${encodeURIComponent(input.boardId!)}?fields=name`,
            )
              .andThen((board) =>
                youtrackClient
                  .fetchSprints(connection, token, input.boardId!)
                  .andThen((sprints) => {
                    const sprint = sprints.find((s) => s.id === input.sprintId)
                    if (!board.name || !sprint) {
                      return errAsync(apiError(404, 'Board or sprint not found'))
                    }
                    return ytSend(connection, token, '/api/commands', {
                      query: `Board ${q(board.name)} ${q(sprint.name)}`,
                      issues: [{ idReadable: key }],
                    })
                  }),
              )
              .orElse((e) => {
                warnings.push(
                  `Task created, but the sprint was not applied: ${taskTrackerErrorMessage(e)}`,
                )
                return okAsync(undefined)
              }),
          )
        }
        const base = connection.baseUrl.replace(/\/$/, '')
        return chain.map(() => ({ key, url: `${base}/issue/${key}`, warnings }))
      })
  },
}
