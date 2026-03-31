import type {
  IssueTrackerConnection,
  IssueTrackerProviderClient,
  TrackerBoard,
  TrackerIssue,
  TrackerSprint,
  TrackerStatus,
} from '../types'

interface JiraIssueFields {
  summary?: string
  description?: string
  status?: { name?: string }
  priority?: { name?: string }
  issuetype?: { name?: string; subtask?: boolean }
  parent?: { key?: string }
  assignee?: { displayName?: string; accountId?: string }
  sprint?: { id?: number; name?: string; state?: string }
}

interface JiraIssue {
  key: string
  fields: JiraIssueFields
  self?: string
}

function buildAuthHeaders(connection: IssueTrackerConnection, token: string): HeadersInit {
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

async function jiraFetch<T>(
  connection: IssueTrackerConnection,
  token: string,
  path: string,
): Promise<T> {
  const url = `${connection.baseUrl.replace(/\/$/, '')}${path}`
  const res = await fetch(url, { headers: buildAuthHeaders(connection, token) })
  if (!res.ok) {
    const body = await res.text().catch(() => '')
    throw new Error(`Jira API error ${res.status}: ${body || res.statusText}`)
  }
  return res.json() as Promise<T>
}

function mapIssueType(fields: JiraIssueFields): string {
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

function mapJiraIssue(issue: JiraIssue, baseUrl: string): TrackerIssue {
  const f = issue.fields
  return {
    key: issue.key,
    summary: f.summary ?? '',
    description: f.description ?? '',
    status: f.status?.name ?? '',
    priority: f.priority?.name ?? '',
    type: mapIssueType(f),
    parentKey: f.parent?.key,
    sprintName: f.sprint?.name,
    sprintNumber: f.sprint?.name ? parseSprintNumber(f.sprint.name) : undefined,
    assignee: f.assignee?.displayName,
    url: `${baseUrl.replace(/\/$/, '')}/browse/${issue.key}`,
  }
}

export const jiraClient: IssueTrackerProviderClient = {
  async testConnection(connection, token) {
    await jiraFetch(connection, token, '/rest/api/3/myself')
    return true
  },

  async getCurrentUserDisplayName(connection, token) {
    const data = await jiraFetch<{ displayName?: string }>(connection, token, '/rest/api/3/myself')
    return data.displayName ?? ''
  },

  async fetchBoards(connection, token) {
    const params = connection.projectKey
      ? `?projectKeyOrId=${encodeURIComponent(connection.projectKey)}`
      : '?maxResults=50'
    const data = await jiraFetch<{
      values: Array<{
        id: number
        name: string
        location?: { projectKey?: string }
      }>
    }>(connection, token, `/rest/agile/1.0/board${params}`)
    return data.values.map(
      (b): TrackerBoard => ({
        id: String(b.id),
        name: b.name,
        projectKey: b.location?.projectKey,
      }),
    )
  },

  async fetchStatuses(connection, token) {
    // Use /rest/api/3/statuses to get all actual issue statuses
    try {
      const data = await jiraFetch<
        Array<{ id: string; name: string; statusCategory?: { key?: string } }>
      >(connection, token, '/rest/api/3/statuses')
      const seen = new Set<string>()
      const statuses: TrackerStatus[] = []
      for (const s of data) {
        if (!seen.has(s.name)) {
          seen.add(s.name)
          statuses.push({ id: s.id, name: s.name })
        }
      }
      return statuses
    } catch {
      // Fallback: try project statuses if available
      if (connection.projectKey) {
        const data = await jiraFetch<Array<{ statuses?: Array<{ id: string; name: string }> }>>(
          connection,
          token,
          `/rest/api/3/project/${encodeURIComponent(connection.projectKey)}/statuses`,
        )
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
      }
      return []
    }
  },

  async fetchIssues(connection, token, params) {
    const resolvedBoardId = params.boardId || connection.boardId
    const fields = 'summary,status,priority,issuetype,parent,assignee,sprint'

    // Board endpoint returns ONLY issues belonging to this board's filter
    if (resolvedBoardId) {
      // Exclude done issues, sort by recent, single request
      const jql = 'statusCategory != Done ORDER BY updated DESC'
      const jqlParam = `&jql=${encodeURIComponent(jql)}`

      const data = await jiraFetch<{ issues: JiraIssue[] }>(
        connection,
        token,
        `/rest/agile/1.0/board/${resolvedBoardId}/issue?fields=${fields}&maxResults=200${jqlParam}`,
      )
      const allIssues = data.issues

      return allIssues.map((i) => mapJiraIssue(i, connection.baseUrl))
    }

    // No board — fallback to JQL search for assigned issues
    const jqlParts: string[] = []
    if (connection.projectKey) {
      jqlParts.push(`project = "${connection.projectKey}"`)
    }
    jqlParts.push('assignee = currentUser()')

    const jql = jqlParts.join(' AND ') + ' ORDER BY updated DESC'
    const data = await jiraFetch<{ issues: JiraIssue[] }>(
      connection,
      token,
      `/rest/api/3/search/jql?jql=${encodeURIComponent(jql)}&fields=${encodeURIComponent(fields)}&maxResults=200`,
    )

    return data.issues.map((i) => mapJiraIssue(i, connection.baseUrl))
  },

  async getCurrentSprint(connection, token, boardId) {
    if (!boardId) {
      const boards = await jiraClient.fetchBoards(connection, token)
      if (boards.length === 0) return null
      boardId = boards[0].id
    }

    const data = await jiraFetch<{
      values: Array<{ id: number; name: string; state: string }>
    }>(connection, token, `/rest/agile/1.0/board/${boardId}/sprint?state=active&maxResults=1`)

    const sprint = data.values[0]
    if (!sprint) return null

    return {
      id: String(sprint.id),
      name: sprint.name,
      number: parseSprintNumber(sprint.name),
      state: sprint.state as TrackerSprint['state'],
    }
  },
}
