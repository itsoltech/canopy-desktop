import type {
  IssueTrackerConnection,
  IssueTrackerProviderClient,
  TrackerBoard,
  TrackerIssue,
  TrackerStatus,
} from '../types'

interface YTIssue {
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

async function ytFetch<T>(
  connection: IssueTrackerConnection,
  token: string,
  path: string,
): Promise<T> {
  const url = `${connection.baseUrl.replace(/\/$/, '')}${path}`
  const res = await fetch(url, { headers: buildHeaders(token) })
  if (!res.ok) {
    const body = await res.text().catch(() => '')
    throw new Error(`YouTrack API error ${res.status}: ${body || res.statusText}`)
  }
  return res.json() as Promise<T>
}

function extractField(issue: YTIssue, fieldName: string): string {
  const field = issue.fields?.find(
    (f) => f.name === fieldName || f.projectCustomField?.field?.name === fieldName,
  )
  if (!field?.value) return ''
  if (Array.isArray(field.value)) return field.value[0]?.name ?? ''
  return field.value.name ?? ''
}

function mapIssueType(typeStr: string): string {
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

function mapYTIssue(issue: YTIssue, baseUrl: string): TrackerIssue {
  const status = extractField(issue, 'State')
  const priority = extractField(issue, 'Priority')
  const typeStr = extractField(issue, 'Type')
  const assignee = extractField(issue, 'Assignee')
  const sprintName = extractField(issue, 'Sprints') || extractField(issue, 'Sprint')
  const parentKey = issue.parent?.issues?.[0]?.idReadable

  return {
    key: issue.idReadable,
    summary: issue.summary ?? '',
    description: issue.description ?? '',
    status,
    priority,
    type: mapIssueType(typeStr),
    parentKey,
    sprintName: sprintName || undefined,
    sprintNumber: sprintName ? parseSprintNumber(sprintName) : undefined,
    assignee: assignee || undefined,
    url: `${baseUrl.replace(/\/$/, '')}/issue/${issue.idReadable}`,
  }
}

export const youtrackClient: IssueTrackerProviderClient = {
  async testConnection(connection, token) {
    await ytFetch(connection, token, '/api/users/me?fields=id,login')
    return true
  },

  async fetchBoards(connection, token) {
    const data = await ytFetch<Array<{ id: string; name: string }>>(
      connection,
      token,
      `/api/agiles?fields=id,name&$top=50`,
    )
    return data.map(
      (b): TrackerBoard => ({
        id: b.id,
        name: b.name,
      }),
    )
  },

  async fetchStatuses(connection, token) {
    const data = await ytFetch<
      Array<{
        id: string
        name: string
        values?: Array<{ name: string }>
      }>
    >(
      connection,
      token,
      `/api/admin/projects/${encodeURIComponent(connection.projectKey)}/customFields?fields=id,name,bundle(values(name))&$top=50`,
    )

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
  },

  async fetchIssues(connection, token, params) {
    const queryParts: string[] = [`project: {${connection.projectKey}}`]

    if (params.statuses && params.statuses.length > 0) {
      const statusList = params.statuses.map((s) => `{${s}}`).join(', ')
      queryParts.push(`State: ${statusList}`)
    }

    if (params.assignedToMe) {
      queryParts.push('for: me')
    }

    const query = queryParts.join(' ')
    const fields =
      'id,idReadable,summary,description,fields(name,projectCustomField(field(name)),value(name,login)),parent(issues(idReadable))'
    const data = await ytFetch<YTIssue[]>(
      connection,
      token,
      `/api/issues?query=${encodeURIComponent(query)}&fields=${encodeURIComponent(fields)}&$top=100`,
    )

    return data.map((i) => mapYTIssue(i, connection.baseUrl))
  },

  async getCurrentSprint(connection, token, boardId) {
    if (!boardId) {
      const boards = await youtrackClient.fetchBoards(connection, token)
      if (boards.length === 0) return null
      boardId = boards[0].id
    }

    const data = await ytFetch<
      Array<{ id: string; name: string; isResolved?: boolean; start?: number; finish?: number }>
    >(
      connection,
      token,
      `/api/agiles/${boardId}/sprints?fields=id,name,isResolved,start,finish&$top=10`,
    )

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
  },
}
