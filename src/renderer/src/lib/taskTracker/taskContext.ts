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

const MAX_DESCRIPTION_LENGTH = 3000
const MAX_COMMENTS = 15
const MAX_COMMENT_LENGTH = 1000

function truncate(text: string, max: number): string {
  if (text.length <= max) return text
  return text.slice(0, max) + '...'
}

export function formatTaskContext(
  task: TaskContextInput,
  comments?: TaskComment[],
  attachments?: TaskAttachmentPath[],
  failedAttachments?: string[],
): string {
  const lines: string[] = ['Work on the following task:', '']

  lines.push(`# ${task.key}: ${task.summary}`)

  const meta: string[] = []
  if (task.status) meta.push(`Status: ${task.status}`)
  if (task.priority) meta.push(`Priority: ${task.priority}`)
  if (task.type) meta.push(`Type: ${task.type}`)
  if (meta.length > 0) lines.push(meta.join(' | '))

  if (task.url) lines.push(`URL: ${task.url}`)

  if (task.description) {
    lines.push('', '## Description', truncate(task.description.trim(), MAX_DESCRIPTION_LENGTH))
  }

  if (comments && comments.length > 0) {
    const recent = comments.slice(-MAX_COMMENTS)
    lines.push('', '## Comments')
    for (const c of recent) {
      const date = c.created ? c.created.slice(0, 10) : ''
      const body = truncate(c.body.trim(), MAX_COMMENT_LENGTH)
      lines.push(`[${date} ${c.author}]: ${body}`)
    }
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
): Promise<string> {
  const [comments, rawAttachments] = await Promise.all([
    window.api.taskTrackerFetchTaskComments(connectionId, task.key).catch(() => []),
    window.api.taskTrackerFetchTaskAttachments(connectionId, task.key).catch(() => []),
  ])

  const attachments: TaskAttachmentPath[] = []
  const failedAttachments: string[] = []
  const downloadResults = await Promise.allSettled(
    rawAttachments.map(async (a) => {
      const localPath = await window.api.taskTrackerDownloadAttachment(connectionId, a.url, a.name)
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

  const context = formatTaskContext(task, comments, attachments, failedAttachments)

  // Schedule cleanup of downloaded attachment files
  if (attachments.length > 0) {
    const paths = attachments.map((a) => a.localPath)
    setTimeout(() => {
      window.api.taskTrackerCleanupAttachments(paths).catch(() => {})
    }, 60_000)
  }

  return context
}
