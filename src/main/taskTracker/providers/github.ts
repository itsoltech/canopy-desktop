import { okAsync, errAsync, type ResultAsync } from 'neverthrow'
import { taskTrackerErrorMessage, type TaskTrackerError } from '../errors'
import type {
  TaskTrackerConnection,
  TaskTrackerProviderClient,
  TrackerAttachment,
  TrackerBoard,
  TrackerComment,
  TrackerTask,
  TrackerSprint,
  TrackerStatus,
  TrackerTransition,
} from '../types'
import { graphqlFetch } from '../../github/graphql'

function apiError(status: number, message: string): TaskTrackerError {
  return { _tag: 'ProviderApiError', status, message, provider: 'github' }
}

function apiUrlForConnection(connection: TaskTrackerConnection): string {
  const baseUrl = connection.baseUrl || 'https://github.com'
  const host = new URL(baseUrl).hostname
  if (host === 'github.com') return 'https://api.github.com/graphql'
  return `https://${host}/api/graphql`
}

function ownerRepo(connection: TaskTrackerConnection): { owner: string; repo: string } {
  const parts = connection.projectKey.split('/')
  return { owner: parts[0], repo: parts[1] }
}

function mapGitHubError<T>(result: ResultAsync<T, unknown>): ResultAsync<T, TaskTrackerError> {
  return result.mapErr((e) => {
    if (e && typeof e === 'object' && '_tag' in e) {
      const gh = e as {
        _tag: string
        status?: number
        message?: string
        errors?: Array<{ message: string }>
      }
      const msg =
        gh.errors?.map((err) => err.message).join(', ') ?? gh.message ?? 'Unknown GitHub error'
      return apiError(gh.status ?? 0, msg)
    }
    return apiError(0, String(e))
  })
}

interface ViewerResponse {
  viewer: { login: string; name: string | null }
}

const ISSUE_ID_QUERY = `
  query ($owner: String!, $name: String!, $number: Int!) {
    repository(owner: $owner, name: $name) {
      issue(number: $number) { id state }
    }
  }
`

const CLOSE_ISSUE_MUTATION = `
  mutation ($id: ID!, $reason: IssueClosedStateReason) {
    closeIssue(input: { issueId: $id, stateReason: $reason }) { issue { id } }
  }
`

const REOPEN_ISSUE_MUTATION = `
  mutation ($id: ID!) {
    reopenIssue(input: { issueId: $id }) { issue { id } }
  }
`

const ADD_COMMENT_MUTATION = `
  mutation ($id: ID!, $body: String!) {
    addComment(input: { subjectId: $id, body: $body }) { clientMutationId }
  }
`

const ASSIGNABLE_USERS_QUERY = `
  query ($owner: String!, $name: String!) {
    repository(owner: $owner, name: $name) {
      assignableUsers(first: 50) { nodes { id login name avatarUrl } }
    }
  }
`

const OPEN_MILESTONES_QUERY = `
  query ($owner: String!, $name: String!) {
    repository(owner: $owner, name: $name) {
      milestones(first: 50, states: OPEN, orderBy: { field: DUE_DATE, direction: ASC }) {
        nodes { id title number }
      }
    }
  }
`

const REPO_ID_QUERY = `
  query ($owner: String!, $name: String!) {
    repository(owner: $owner, name: $name) { id }
  }
`

const CREATE_ISSUE_MUTATION = `
  mutation ($input: CreateIssueInput!) {
    createIssue(input: $input) { issue { number url } }
  }
`

/** Resolve the GraphQL node id (needed by mutations) + current state for an issue number. */
function fetchIssueId(
  connection: TaskTrackerConnection,
  token: string,
  taskKey: string,
): ResultAsync<{ id: string; state: string }, TaskTrackerError> {
  const apiUrl = apiUrlForConnection(connection)
  const { owner, repo } = ownerRepo(connection)
  const issueNumber = parseInt(taskKey.replace(/^#/, ''), 10)
  if (isNaN(issueNumber)) return errAsync(apiError(0, `Invalid issue number: ${taskKey}`))
  return mapGitHubError(
    graphqlFetch<{ repository: { issue: { id: string; state: string } | null } }>(
      apiUrl,
      token,
      ISSUE_ID_QUERY,
      { owner, name: repo, number: issueNumber },
    ),
  ).andThen((data) =>
    data.repository.issue
      ? okAsync(data.repository.issue)
      : errAsync(apiError(404, `Issue ${taskKey} not found`)),
  )
}

interface IssuesResponse {
  repository: {
    issues: {
      nodes: Array<{
        number: number
        title: string
        body: string
        state: string
        url: string
        labels: { nodes: Array<{ name: string; color: string }> }
        assignees: { nodes: Array<{ login: string }> }
        milestone: { title: string; number: number } | null
        author: { login: string } | null
      }>
    }
    labels: { nodes: Array<{ name: string; color: string }> }
    milestones: { nodes: Array<{ title: string; number: number; state: string }> }
  }
}

interface CommentsResponse {
  repository: {
    issue: {
      comments: {
        nodes: Array<{
          id: string
          body: string
          author: { login: string } | null
          createdAt: string
        }>
      }
    }
  }
}

function mapTaskType(labels: Array<{ name: string }>): string {
  for (const label of labels) {
    const lower = label.name.toLowerCase()
    if (lower.startsWith('type:') || lower.startsWith('kind:')) {
      const type = lower.split(':')[1].trim()
      if (type === 'bug' || type === 'fix') return 'bug'
      if (type === 'feature' || type === 'enhancement') return 'story'
      if (type === 'epic') return 'epic'
      return type
    }
    if (lower === 'bug') return 'bug'
    if (lower === 'enhancement' || lower === 'feature') return 'story'
    if (lower === 'epic') return 'epic'
  }
  return 'task'
}

function mapPriority(labels: Array<{ name: string }>): string {
  for (const label of labels) {
    const lower = label.name.toLowerCase()
    if (lower.startsWith('priority:') || lower.startsWith('p:')) {
      return lower.split(':')[1].trim()
    }
    if (lower === 'critical' || lower === 'urgent') return 'critical'
    if (lower === 'high') return 'high'
    if (lower === 'low') return 'low'
  }
  return 'medium'
}

const ISSUES_QUERY = `
query($owner: String!, $name: String!, $first: Int!, $states: [IssueState!], $filterBy: IssueFilters) {
  repository(owner: $owner, name: $name) {
    issues(first: $first, states: $states, filterBy: $filterBy, orderBy: {field: UPDATED_AT, direction: DESC}) {
      nodes {
        number, title, body, state, url
        labels(first: 10) { nodes { name, color } }
        assignees(first: 3) { nodes { login } }
        milestone { title, number }
        author { login }
      }
    }
    labels(first: 50) { nodes { name, color } }
    milestones(first: 20, states: OPEN) { nodes { title, number, state } }
  }
}
`

const COMMENTS_QUERY = `
query($owner: String!, $name: String!, $number: Int!) {
  repository(owner: $owner, name: $name) {
    issue(number: $number) {
      comments(first: 50, orderBy: {field: UPDATED_AT, direction: ASC}) {
        nodes {
          id, body
          author { login }
          createdAt
        }
      }
    }
  }
}
`

export const githubClient: TaskTrackerProviderClient = {
  testConnection(connection, token) {
    const apiUrl = apiUrlForConnection(connection)
    return mapGitHubError(graphqlFetch<ViewerResponse>(apiUrl, token, '{ viewer { login } }')).map(
      () => true,
    )
  },

  getCurrentUserDisplayName(connection, token) {
    const apiUrl = apiUrlForConnection(connection)
    return mapGitHubError(
      graphqlFetch<ViewerResponse>(apiUrl, token, '{ viewer { login, name } }'),
    ).map((data) => data.viewer.name ?? data.viewer.login)
  },

  fetchTaskByKey(connection, token, taskKey) {
    const apiUrl = apiUrlForConnection(connection)
    const { owner, repo } = ownerRepo(connection)
    const issueNumber = parseInt(taskKey.replace(/^#/, ''), 10)
    if (isNaN(issueNumber)) return okAsync(null)

    const query = `
      query($owner: String!, $name: String!, $number: Int!) {
        repository(owner: $owner, name: $name) {
          issue(number: $number) {
            number, title, body, state, url
            labels(first: 10) { nodes { name, color } }
            assignees(first: 3) { nodes { login } }
            milestone { title, number }
            author { login }
          }
        }
      }
    `

    return mapGitHubError(
      graphqlFetch<{
        repository: {
          issue: {
            number: number
            title: string
            body: string
            state: string
            url: string
            labels: { nodes: Array<{ name: string; color: string }> }
            assignees: { nodes: Array<{ login: string }> }
            milestone: { title: string; number: number } | null
            author: { login: string } | null
          }
        }
      }>(apiUrl, token, query, { owner, name: repo, number: issueNumber }),
    ).map((data) => {
      const issue = data.repository.issue
      if (!issue) return null
      return {
        key: `#${issue.number}`,
        summary: issue.title,
        description: issue.body ?? '',
        status: issue.state.toLowerCase(),
        priority: mapPriority(issue.labels.nodes),
        type: mapTaskType(issue.labels.nodes),
        assignee: issue.assignees.nodes[0]?.login,
        sprintName: issue.milestone?.title,
        sprintNumber: issue.milestone?.number,
        url: issue.url,
      } as TrackerTask | null
    })
  },

  fetchBoards(connection) {
    const { owner, repo } = ownerRepo(connection)
    return okAsync([{ id: 'repo', name: `${owner}/${repo}` }] satisfies TrackerBoard[])
  },

  fetchProjects() {
    // GitHub issue keys (#123) carry no project prefix, so per-project template overrides can
    // never match them — expose no projects rather than offer overrides that would never apply.
    return okAsync([])
  },

  fetchTaskTypes() {
    return okAsync(['issue'])
  },

  fetchStatuses() {
    return okAsync([
      { id: 'OPEN', name: 'Open', statusCategory: 'todo' },
      { id: 'CLOSED', name: 'Closed', statusCategory: 'done' },
    ] satisfies TrackerStatus[])
  },

  fetchTasks(connection, token, params) {
    const apiUrl = apiUrlForConnection(connection)
    const { owner, repo } = ownerRepo(connection)

    const states: string[] = []
    if (params.statuses?.length) {
      for (const s of params.statuses) {
        if (s.toUpperCase() === 'OPEN' || s.toUpperCase() === 'CLOSED') {
          states.push(s.toUpperCase())
        }
      }
    }
    if (states.length === 0) states.push('OPEN')

    const filterBy: Record<string, unknown> = {}
    if (params.assignedToMe) {
      filterBy.assignee = '*'
    }

    return mapGitHubError(
      graphqlFetch<IssuesResponse>(apiUrl, token, ISSUES_QUERY, {
        owner,
        name: repo,
        first: 100,
        states,
        filterBy: Object.keys(filterBy).length > 0 ? filterBy : undefined,
      }),
    ).map((data) =>
      data.repository.issues.nodes.map((issue): TrackerTask => ({
        key: `#${issue.number}`,
        summary: issue.title,
        description: issue.body ?? '',
        status: issue.state.toLowerCase(),
        priority: mapPriority(issue.labels.nodes),
        type: mapTaskType(issue.labels.nodes),
        assignee: issue.assignees.nodes[0]?.login,
        sprintName: issue.milestone?.title,
        sprintNumber: issue.milestone?.number,
        url: issue.url,
      })),
    )
  },

  getCurrentSprint(connection, token) {
    const apiUrl = apiUrlForConnection(connection)
    const { owner, repo } = ownerRepo(connection)

    const query = `
      query($owner: String!, $name: String!) {
        repository(owner: $owner, name: $name) {
          milestones(first: 1, states: OPEN, orderBy: {field: DUE_DATE, direction: ASC}) {
            nodes { title, number, state }
          }
        }
      }
    `

    return mapGitHubError(
      graphqlFetch<{
        repository: {
          milestones: { nodes: Array<{ title: string; number: number; state: string }> }
        }
      }>(apiUrl, token, query, { owner, name: repo }),
    ).map((data) => {
      const ms = data.repository.milestones.nodes[0]
      if (!ms) return null
      return {
        id: String(ms.number),
        name: ms.title,
        number: ms.number,
        state: 'active' as const,
      } satisfies TrackerSprint
    })
  },

  fetchTaskComments(connection, token, taskKey) {
    const apiUrl = apiUrlForConnection(connection)
    const { owner, repo } = ownerRepo(connection)
    const issueNumber = parseInt(taskKey.replace(/^#/, ''), 10)
    if (isNaN(issueNumber)) return okAsync([])

    return mapGitHubError(
      graphqlFetch<CommentsResponse>(apiUrl, token, COMMENTS_QUERY, {
        owner,
        name: repo,
        number: issueNumber,
      }),
    ).map((data) =>
      data.repository.issue.comments.nodes.map((c): TrackerComment => ({
        id: c.id,
        author: c.author?.login ?? '',
        body: c.body,
        created: c.createdAt,
      })),
    )
  },

  fetchTaskAttachments() {
    return okAsync([] satisfies TrackerAttachment[])
  },

  fetchTransitions(connection, token, taskKey) {
    // GitHub issues have no workflow — the only "transitions" are close (with a state reason,
    // GitHub's equivalent of a resolution) and reopen.
    return fetchIssueId(connection, token, taskKey).map((issue): TrackerTransition[] =>
      issue.state === 'OPEN'
        ? [
            {
              id: 'close:COMPLETED',
              name: 'Close (completed)',
              toStatus: 'closed',
              toStatusCategory: 'done',
              fields: [],
            },
            {
              id: 'close:NOT_PLANNED',
              name: 'Close (not planned)',
              toStatus: 'closed',
              toStatusCategory: 'done',
              fields: [],
            },
          ]
        : [
            {
              id: 'reopen',
              name: 'Reopen',
              toStatus: 'open',
              toStatusCategory: 'todo',
              fields: [],
            },
          ],
    )
  },

  applyTransition(connection, token, taskKey, transitionId, opts) {
    const apiUrl = apiUrlForConnection(connection)
    // The transition id crosses the untrusted IPC boundary — only the three states this provider
    // actually offers may reach a mutation, and each must match the issue's CURRENT state (a typo
    // like "reopne" must fail loudly, not silently close the issue as COMPLETED).
    const KNOWN_TRANSITIONS: Record<string, { mutation: 'reopen' | 'close'; reason?: string }> = {
      reopen: { mutation: 'reopen' },
      'close:COMPLETED': { mutation: 'close', reason: 'COMPLETED' },
      'close:NOT_PLANNED': { mutation: 'close', reason: 'NOT_PLANNED' },
    }
    const transition = KNOWN_TRANSITIONS[transitionId]
    if (!transition) {
      return errAsync(apiError(400, `Unknown transition: ${transitionId}`))
    }
    return fetchIssueId(connection, token, taskKey)
      .andThen((issue) => {
        if (transition.mutation === 'reopen') {
          if (issue.state !== 'CLOSED') {
            return errAsync(apiError(400, `Cannot reopen an issue in state ${issue.state}`))
          }
          return mapGitHubError(
            graphqlFetch<unknown>(apiUrl, token, REOPEN_ISSUE_MUTATION, { id: issue.id }),
          ).map(() => issue.id)
        }
        if (issue.state !== 'OPEN') {
          return errAsync(apiError(400, `Cannot close an issue in state ${issue.state}`))
        }
        return mapGitHubError(
          graphqlFetch<unknown>(apiUrl, token, CLOSE_ISSUE_MUTATION, {
            id: issue.id,
            reason: transition.reason,
          }),
        ).map(() => issue.id)
      })
      .andThen((issueId) => {
        if (!opts.comment) return okAsync(undefined)
        // The state change already happened — a failed comment must not fail the transition,
        // or the caller would retry a close/reopen that has in fact been applied.
        return mapGitHubError(
          graphqlFetch<unknown>(apiUrl, token, ADD_COMMENT_MUTATION, {
            id: issueId,
            body: opts.comment,
          }),
        ).orElse((err) => {
          console.warn(
            '[github] transition applied but comment failed:',
            taskTrackerErrorMessage(err),
          )
          return okAsync(undefined)
        })
      })
      .map(() => undefined)
  },

  addComment(connection, token, taskKey, body) {
    const apiUrl = apiUrlForConnection(connection)
    return fetchIssueId(connection, token, taskKey).andThen((issue) =>
      mapGitHubError(
        graphqlFetch<unknown>(apiUrl, token, ADD_COMMENT_MUTATION, { id: issue.id, body }),
      ).map(() => undefined),
    )
  },

  fetchAssignableUsers(connection, token) {
    const apiUrl = apiUrlForConnection(connection)
    const { owner, repo } = ownerRepo(connection)
    return mapGitHubError(
      graphqlFetch<{
        repository: {
          assignableUsers: {
            nodes: Array<{ id: string; login: string; name: string | null; avatarUrl?: string }>
          }
        }
      }>(apiUrl, token, ASSIGNABLE_USERS_QUERY, { owner, name: repo }),
    ).map((data) =>
      data.repository.assignableUsers.nodes.map((u) => ({
        id: u.id,
        displayName: u.name ?? u.login,
        avatarUrl: u.avatarUrl,
      })),
    )
  },

  fetchSprints(connection, token) {
    // Open milestones stand in for sprints. `id` is the GraphQL NODE id (what createIssue
    // takes as milestoneId) — unlike getCurrentSprint, which exposes the numeric number.
    const apiUrl = apiUrlForConnection(connection)
    const { owner, repo } = ownerRepo(connection)
    return mapGitHubError(
      graphqlFetch<{
        repository: { milestones: { nodes: Array<{ id: string; title: string; number: number }> } }
      }>(apiUrl, token, OPEN_MILESTONES_QUERY, { owner, name: repo }),
    ).map((data) =>
      data.repository.milestones.nodes.map((m): TrackerSprint => ({
        id: m.id,
        name: m.title,
        number: m.number,
        state: 'active',
      })),
    )
  },

  fetchCreateTaskTypes() {
    // Issues have no type concept — the create form hides the field.
    return okAsync([] as Array<{ name: string; iconUrl?: string }>)
  },

  createTask(connection, token, input) {
    const apiUrl = apiUrlForConnection(connection)
    const { owner, repo } = ownerRepo(connection)
    return mapGitHubError(
      graphqlFetch<{ repository: { id: string } | null }>(apiUrl, token, REPO_ID_QUERY, {
        owner,
        name: repo,
      }),
    )
      .andThen((data) =>
        data.repository
          ? okAsync(data.repository.id)
          : errAsync(apiError(404, `Repository ${owner}/${repo} not found`)),
      )
      .andThen((repositoryId) =>
        mapGitHubError(
          graphqlFetch<{ createIssue: { issue: { number: number; url: string } | null } }>(
            apiUrl,
            token,
            CREATE_ISSUE_MUTATION,
            {
              input: {
                repositoryId,
                title: input.title,
                ...(input.description ? { body: input.description } : {}),
                ...(input.assigneeId ? { assigneeIds: [input.assigneeId] } : {}),
                ...(input.sprintId ? { milestoneId: input.sprintId } : {}),
              },
            },
          ),
        ),
      )
      .andThen((data) => {
        const issue = data.createIssue.issue
        if (!issue) return errAsync(apiError(0, 'GitHub did not return the created issue'))
        // The GitHub API has no issue-attachment upload — pasted images cannot be carried over.
        const warnings =
          (input.attachments?.length ?? 0) > 0
            ? ['Task created, but GitHub does not support image attachments via the API']
            : []
        return okAsync({ key: `#${issue.number}`, url: issue.url, warnings })
      })
  },
}
