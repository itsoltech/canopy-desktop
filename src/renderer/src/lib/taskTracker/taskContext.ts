interface TaskContextInput {
  key: string
  summary: string
  description: string
  status: string
  priority: string
  type: string
  url?: string
}

interface TaskComment {
  author: string
  body: string
  created: string
}

interface TaskAttachmentPath {
  name: string
  localPath: string
}

// Description and comment bodies are sent without a character cap — the agent manages
// its own context window and will summarise or ignore content as needed.
const MAX_COMMENTS = 15

function normalizeTaskText(text: string): string {
  return text.replace(/\r\n?/g, '\n').replace(/\u2028|\u2029/g, '\n')
}

function formatMultilineText(text: string): string {
  return normalizeTaskText(text)
    .replace(/[ \t]+\n/g, '\n')
    .replace(/\n{3,}/g, '\n\n')
    .trim()
}

function pushMultiline(lines: string[], text: string): void {
  for (const line of text.split('\n')) {
    lines.push(line)
  }
}

export function formatTaskContext(
  task: TaskContextInput,
  comments?: TaskComment[],
  attachments?: TaskAttachmentPath[],
  failedAttachments?: string[],
): string {
  const lines: string[] = ['Work on the following task:', '']

  lines.push(`# ${task.key}: ${normalizeTaskText(task.summary).trim()}`)

  const meta: string[] = []
  if (task.status) meta.push(`Status: ${task.status}`)
  if (task.priority) meta.push(`Priority: ${task.priority}`)
  if (task.type) meta.push(`Type: ${task.type}`)
  if (meta.length > 0) lines.push(meta.join(' | '))

  if (task.url) lines.push(`URL: ${task.url}`)

  if (task.description) {
    const description = formatMultilineText(task.description)
    if (description) {
      lines.push('', '## Description')
      pushMultiline(lines, description)
    }
  }

  if (comments && comments.length > 0) {
    const recent = comments.slice(-MAX_COMMENTS)
    lines.push('', '## Comments')
    for (const c of recent) {
      const date = c.created ? c.created.slice(0, 10) : ''
      const author = normalizeTaskText(c.author).trim()
      const body = formatMultilineText(c.body)
      const header = [date, author].filter(Boolean).join(' ')
      lines.push(header ? `[${header}]` : '[Comment]')
      if (body) pushMultiline(lines, body)
      lines.push('')
    }
    if (lines.at(-1) === '') lines.pop()
  }

  if (
    (attachments && attachments.length > 0) ||
    (failedAttachments && failedAttachments.length > 0)
  ) {
    lines.push('', '## Attachments')
    if (attachments) {
      for (const a of attachments) {
        lines.push(`@${a.localPath}`)
      }
    }
    if (failedAttachments && failedAttachments.length > 0) {
      lines.push(`(failed to download: ${failedAttachments.join(', ')})`)
    }
  }

  return lines.join('\n')
}

export async function fetchAndFormatTaskContext(
  connectionId: string,
  task: TaskContextInput,
  repoRoot?: string,
): Promise<string> {
  const [fullTask, comments, rawAttachments] = await Promise.all([
    // Re-fetch full task to get description (list fetch omits it for performance)
    window.api
      .trackerConfigFindTaskByKey(repoRoot, task.key, connectionId)
      .catch(() => window.api.taskTrackerFindTaskByKey(task.key).catch(() => null)),
    window.api
      .trackerConfigFetchTaskComments(repoRoot, task.key, connectionId)
      .catch(() => window.api.taskTrackerFetchTaskComments(connectionId, task.key).catch(() => [])),
    window.api
      .trackerConfigFetchTaskAttachments(repoRoot, task.key, connectionId)
      .catch(() =>
        window.api.taskTrackerFetchTaskAttachments(connectionId, task.key).catch(() => []),
      ),
  ])

  const resolvedTask: TaskContextInput = fullTask
    ? { ...task, description: fullTask.description || task.description }
    : task

  const attachments: TaskAttachmentPath[] = []
  const failedAttachments: string[] = []
  const downloadResults = await Promise.allSettled(
    rawAttachments.map(async (a) => {
      const localPath = await window.api
        .trackerConfigDownloadAttachment(repoRoot, a.url, a.name, connectionId)
        .catch(() => window.api.taskTrackerDownloadAttachment(connectionId, a.url, a.name))
      return { name: a.name, localPath }
    }),
  )
  for (let i = 0; i < downloadResults.length; i++) {
    const result = downloadResults[i]
    if (result.status === 'fulfilled') {
      attachments.push(result.value)
    } else {
      failedAttachments.push(rawAttachments[i].name)
    }
  }

  const context = formatTaskContext(resolvedTask, comments, attachments, failedAttachments)

  // Schedule cleanup of downloaded attachment files
  if (attachments.length > 0) {
    const paths = attachments.map((a) => a.localPath)
    setTimeout(() => {
      window.api.taskTrackerCleanupAttachments(paths).catch(() => {})
    }, 60_000)
  }

  return context
}
