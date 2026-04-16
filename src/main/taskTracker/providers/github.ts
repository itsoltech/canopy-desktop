import { okAsync, type ResultAsync } from 'neverthrow'
import type { TaskTrackerError } from '../errors'
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

  fetchStatuses() {
    return okAsync([
      { id: 'OPEN', name: 'Open' },
      { id: 'CLOSED', name: 'Closed' },
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
      data.repository.issues.nodes.map(
        (issue): TrackerTask => ({
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
        }),
      ),
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
      data.repository.issue.comments.nodes.map(
        (c): TrackerComment => ({
          id: c.id,
          author: c.author?.login ?? '',
          body: c.body,
          created: c.createdAt,
        }),
      ),
    )
  },

  fetchTaskAttachments() {
    return okAsync([] satisfies TrackerAttachment[])
  },
}
