import { join, basename } from 'path'
import { mkdirSync, createWriteStream, rmSync } from 'fs'
import os from 'os'
import { randomUUID } from 'crypto'
import { pipeline } from 'stream/promises'
import { Readable } from 'stream'
import { ok, err, okAsync, errAsync, type Result, type ResultAsync } from 'neverthrow'
import type { PreferencesStore } from '../db/PreferencesStore'
import type { KeychainTokenStore } from './KeychainTokenStore'
import type { TaskTrackerError } from './errors'
import { fromExternalCall, errorMessage } from '../errors'
import { createProviderClient } from './providers'
import { GitRepository } from '../git/GitRepository'
import { parseGitHubRemote } from '../github/remoteUrl'
import type {
  TaskTrackerConnection,
  TrackerConfig,
  RepoConfig,
  TrackerAttachment,
  TrackerBoard,
  TrackerComment,
  TrackerTask,
  TrackerSprint,
  TrackerStatus,
} from './types'

const CONNECTIONS_PREF_KEY = 'taskTracker.connections'

export class TaskTrackerManager {
  constructor(
    private preferencesStore: PreferencesStore,
    private keychainTokenStore?: KeychainTokenStore,
  ) {}

  // --- Config-based methods ---

  private findTracker(config: RepoConfig, trackerId?: string): TrackerConfig | undefined {
    if (config.trackers.length === 0) return undefined
    if (trackerId) return config.trackers.find((t) => t.id === trackerId) ?? config.trackers[0]
    return config.trackers[0]
  }

  private buildConnectionFromTracker(
    tracker: TrackerConfig,
    projectKey?: string,
  ): TaskTrackerConnection {
    const creds = this.keychainTokenStore?.getCredentials(tracker.provider, tracker.baseUrl)
    return {
      id: tracker.id,
      provider: tracker.provider,
      name: `${tracker.provider}:${tracker.baseUrl}`,
      baseUrl: tracker.baseUrl,
      projectKey: projectKey ?? tracker.projectKey ?? '',
      authPrefKey: '',
      username: creds?.username,
    }
  }

  private getTokenFromTracker(tracker: TrackerConfig): Result<string, TaskTrackerError> {
    if (!this.keychainTokenStore) {
      return err({ _tag: 'AuthTokenMissing', connectionName: tracker.baseUrl })
    }
    const creds = this.keychainTokenStore.getCredentials(tracker.provider, tracker.baseUrl)
    if (!creds) {
      return err({ _tag: 'AuthTokenMissing', connectionName: tracker.baseUrl })
    }
    return ok(creds.token)
  }

  private resolveConfigConnection(
    config: RepoConfig,
    trackerId?: string,
    projectKey?: string,
  ): Result<{ conn: TaskTrackerConnection; token: string }, TaskTrackerError> {
    const tracker = this.findTracker(config, trackerId)
    if (!tracker) {
      return err({ _tag: 'ConfigNotFound', repoRoot: 'no trackers configured' })
    }
    const conn = this.buildConnectionFromTracker(tracker, projectKey)
    return this.getTokenFromTracker(tracker).map((token) => ({ conn, token }))
  }

  getConnectionFromConfig(
    config: RepoConfig,
    trackerId?: string,
    projectKey?: string,
  ): TaskTrackerConnection | null {
    const tracker = this.findTracker(config, trackerId)
    if (!tracker) return null
    return this.buildConnectionFromTracker(tracker, projectKey)
  }

  testConnectionFromConfig(config: RepoConfig): ResultAsync<boolean, TaskTrackerError> {
    return this.resolveConfigConnection(config).asyncAndThen(({ conn, token }) => {
      const client = createProviderClient(conn.provider)
      return client.testConnection(conn, token)
    })
  }

  testNewConnectionFromConfig(
    config: RepoConfig,
    token: string,
    trackerId?: string,
  ): ResultAsync<boolean, TaskTrackerError> {
    const tracker = this.findTracker(config, trackerId)
    if (!tracker) return errAsync({ _tag: 'ConfigNotFound', repoRoot: 'no trackers' })
    const conn = this.buildConnectionFromTracker(tracker)
    const client = createProviderClient(conn.provider)
    return client.testConnection(conn, token)
  }

  fetchBoardsFromConfig(config: RepoConfig): ResultAsync<TrackerBoard[], TaskTrackerError> {
    return this.resolveConfigConnection(config).asyncAndThen(({ conn, token }) => {
      const client = createProviderClient(conn.provider)
      return client.fetchBoards(conn, token)
    })
  }

  fetchBoardsForNewFromConfig(
    config: RepoConfig,
    token: string,
    trackerId?: string,
  ): ResultAsync<TrackerBoard[], TaskTrackerError> {
    const tracker = this.findTracker(config, trackerId)
    if (!tracker) return errAsync({ _tag: 'ConfigNotFound', repoRoot: 'no trackers' })
    const conn = this.buildConnectionFromTracker(tracker)
    const client = createProviderClient(conn.provider)
    return client.fetchBoards(conn, token)
  }

  fetchStatusesFromConfig(
    config: RepoConfig,
    boardId?: string,
  ): ResultAsync<TrackerStatus[], TaskTrackerError> {
    return this.resolveConfigConnection(config).asyncAndThen(({ conn, token }) => {
      const client = createProviderClient(conn.provider)
      return client.fetchStatuses(conn, token, boardId)
    })
  }

  fetchTasksFromConfig(
    config: RepoConfig,
    params: { statuses?: string[]; assignedToMe?: boolean; boardId?: string },
  ): ResultAsync<TrackerTask[], TaskTrackerError> {
    return this.resolveConfigConnection(config).asyncAndThen(({ conn, token }) => {
      const client = createProviderClient(conn.provider)
      return client.fetchTasks(conn, token, params)
    })
  }

  getCurrentSprintFromConfig(
    config: RepoConfig,
    boardId?: string,
  ): ResultAsync<TrackerSprint | null, TaskTrackerError> {
    return this.resolveConfigConnection(config).asyncAndThen(({ conn, token }) => {
      const client = createProviderClient(conn.provider)
      return client.getCurrentSprint(conn, token, boardId)
    })
  }

  getCurrentUserFromConfig(config: RepoConfig): ResultAsync<string, TaskTrackerError> {
    return this.resolveConfigConnection(config).asyncAndThen(({ conn, token }) => {
      const client = createProviderClient(conn.provider)
      return client.getCurrentUserDisplayName(conn, token)
    })
  }

  fetchTaskCommentsFromConfig(
    config: RepoConfig,
    taskKey: string,
  ): ResultAsync<TrackerComment[], TaskTrackerError> {
    return this.resolveConfigConnection(config).asyncAndThen(({ conn, token }) => {
      const client = createProviderClient(conn.provider)
      return client.fetchTaskComments(conn, token, taskKey)
    })
  }

  fetchTaskAttachmentsFromConfig(
    config: RepoConfig,
    taskKey: string,
  ): ResultAsync<TrackerAttachment[], TaskTrackerError> {
    return this.resolveConfigConnection(config).asyncAndThen(({ conn, token }) => {
      const client = createProviderClient(conn.provider)
      return client.fetchTaskAttachments(conn, token, taskKey)
    })
  }

  findTaskByKeyFromConfig(
    config: RepoConfig,
    taskKey: string,
  ): ResultAsync<TrackerTask | null, TaskTrackerError> {
    return this.resolveConfigConnection(config).asyncAndThen(({ conn, token }) => {
      const client = createProviderClient(conn.provider)
      return client.fetchTaskByKey(conn, token, taskKey)
    })
  }

  downloadAttachmentFromConfig(
    config: RepoConfig,
    url: string,
    filename: string,
  ): ResultAsync<string, TaskTrackerError> {
    const dlErr = (reason: string): TaskTrackerError => ({
      _tag: 'AttachmentDownloadFailed',
      filename,
      reason,
    })

    return this.resolveConfigConnection(config)
      .andThen(({ conn, token }) => {
        const connBase = conn.baseUrl.replace(/\/$/, '')
        if (!url.startsWith(connBase)) {
          return err(dlErr('URL does not match connection base URL'))
        }
        return ok({ conn, token })
      })
      .asyncAndThen(({ conn, token }) => this.downloadToTempDir(url, filename, conn, token, dlErr))
  }

  private downloadToTempDir(
    url: string,
    filename: string,
    conn: TaskTrackerConnection,
    token: string,
    dlErr: (reason: string) => TaskTrackerError,
  ): ResultAsync<string, TaskTrackerError> {
    const dir = join(os.tmpdir(), `canopy-attachments-${randomUUID()}`)
    mkdirSync(dir, { recursive: true })

    const safeName = basename(filename.replace(/[/\\]/g, '_'))
    const filePath = join(dir, safeName)

    const headers: Record<string, string> = {
      Authorization: conn.username
        ? `Basic ${Buffer.from(`${conn.username}:${token}`).toString('base64')}`
        : `Bearer ${token}`,
    }

    const MAX_ATTACHMENT_BYTES = 50 * 1024 * 1024

    return fromExternalCall(fetch(url, { headers, signal: AbortSignal.timeout(60_000) }), (e) =>
      dlErr(errorMessage(e)),
    )
      .andThen((res) => {
        if (!res.ok) return errAsync(dlErr(`HTTP ${res.status}`))
        if (!res.body) return errAsync(dlErr('Empty response body'))
        const contentLength = Number(res.headers.get('content-length') || 0)
        if (contentLength > MAX_ATTACHMENT_BYTES) {
          return errAsync(dlErr(`Attachment too large: ${contentLength} bytes`))
        }
        const nodeStream = Readable.fromWeb(res.body as import('stream/web').ReadableStream)
        return fromExternalCall(pipeline(nodeStream, createWriteStream(filePath)), (e) =>
          dlErr(errorMessage(e)),
        ).map(() => filePath)
      })
      .mapErr((error) => {
        try {
          rmSync(dir, { recursive: true, force: true })
        } catch {
          // Ignore cleanup errors
        }
        return error
      })
  }

  // --- Legacy connection-based methods (to be removed in Phase 7) ---

  getConnections(): TaskTrackerConnection[] {
    const raw = this.preferencesStore.get(CONNECTIONS_PREF_KEY)
    if (!raw) return []
    try {
      return JSON.parse(raw) as TaskTrackerConnection[]
    } catch {
      return []
    }
  }

  private saveConnections(connections: TaskTrackerConnection[]): void {
    this.preferencesStore.set(CONNECTIONS_PREF_KEY, JSON.stringify(connections))
  }

  private getConnection(connectionId: string): Result<TaskTrackerConnection, TaskTrackerError> {
    const conn = this.getConnections().find((c) => c.id === connectionId)
    if (!conn) return err({ _tag: 'ConnectionNotFound', connectionId })
    return ok(conn)
  }

  private getToken(connection: TaskTrackerConnection): Result<string, TaskTrackerError> {
    const token = this.preferencesStore.get(connection.authPrefKey)
    if (!token) return err({ _tag: 'AuthTokenMissing', connectionName: connection.name })
    return ok(token)
  }

  private resolveGitHubConnection(
    conn: TaskTrackerConnection,
    repoRoot?: string,
  ): ResultAsync<TaskTrackerConnection, TaskTrackerError> {
    if (conn.provider !== 'github' || conn.projectKey) return okAsync(conn)
    if (!repoRoot) {
      return errAsync({
        _tag: 'ProviderApiError',
        status: 0,
        message: 'Repository not configured and no workspace to auto-detect from',
        provider: 'github',
      })
    }
    return GitRepository.getRemoteUrl(repoRoot)
      .mapErr(
        (): TaskTrackerError => ({
          _tag: 'ProviderApiError',
          status: 0,
          message: 'Could not read git remote URL from workspace',
          provider: 'github',
        }),
      )
      .andThen((url) => {
        const parsed = parseGitHubRemote(url)
        if (parsed.isErr()) {
          return errAsync<TaskTrackerConnection, TaskTrackerError>({
            _tag: 'ProviderApiError',
            status: 0,
            message: 'Workspace remote is not a GitHub repository',
            provider: 'github',
          })
        }
        const { owner, repo, host } = parsed.value
        return okAsync({
          ...conn,
          projectKey: `${owner}/${repo}`,
          baseUrl: conn.baseUrl || `https://${host}`,
        })
      })
  }

  addConnection(
    connection: Omit<TaskTrackerConnection, 'id' | 'authPrefKey'>,
    token: string,
  ): TaskTrackerConnection {
    const id = crypto.randomUUID()
    const authPrefKey = `taskTracker.token.${id}`

    const newConn: TaskTrackerConnection = {
      ...connection,
      id,
      authPrefKey,
    }

    this.preferencesStore.set(authPrefKey, token)

    const connections = this.getConnections()
    connections.push(newConn)
    this.saveConnections(connections)

    return newConn
  }

  updateConnection(
    connectionId: string,
    updates: Partial<Omit<TaskTrackerConnection, 'id' | 'authPrefKey'>>,
    newToken?: string,
  ): TaskTrackerConnection | null {
    const connections = this.getConnections()
    const idx = connections.findIndex((c) => c.id === connectionId)
    if (idx < 0) return null

    const conn = connections[idx]
    connections[idx] = { ...conn, ...updates }

    if (newToken) {
      this.preferencesStore.set(conn.authPrefKey, newToken)
    }

    this.saveConnections(connections)
    return connections[idx]
  }

  removeConnection(connectionId: string): void {
    const connections = this.getConnections()
    const conn = connections.find((c) => c.id === connectionId)
    if (conn) {
      this.preferencesStore.delete(conn.authPrefKey)
    }
    this.saveConnections(connections.filter((c) => c.id !== connectionId))
  }

  testConnection(connectionId: string): ResultAsync<boolean, TaskTrackerError> {
    return this.getConnection(connectionId)
      .andThen((conn) => this.getToken(conn).map((token) => ({ conn, token })))
      .asyncAndThen(({ conn, token }) => {
        const client = createProviderClient(conn.provider)
        return client.testConnection(conn, token)
      })
  }

  testNewConnection(
    connection: Omit<TaskTrackerConnection, 'id' | 'authPrefKey'>,
    token: string,
  ): ResultAsync<boolean, TaskTrackerError> {
    const tempConn: TaskTrackerConnection = {
      ...connection,
      id: 'temp',
      authPrefKey: 'temp',
    }
    const client = createProviderClient(connection.provider)
    return client.testConnection(tempConn, token)
  }

  fetchBoardsForNew(
    connection: Omit<TaskTrackerConnection, 'id' | 'authPrefKey'>,
    token: string,
  ): ResultAsync<TrackerBoard[], TaskTrackerError> {
    const tempConn: TaskTrackerConnection = {
      ...connection,
      id: 'temp',
      authPrefKey: 'temp',
    }
    const client = createProviderClient(connection.provider)
    return client.fetchBoards(tempConn, token)
  }

  async findTaskByKey(taskKey: string): Promise<TrackerTask | null> {
    const connections = this.getConnections()
    for (const conn of connections) {
      const token = this.getToken(conn)
      if (token.isErr()) continue
      const client = createProviderClient(conn.provider)
      const result = await client.fetchTaskByKey(conn, token.value, taskKey)
      if (result.isErr()) continue
      if (result.value) return result.value
    }
    return null
  }

  getCurrentUserDisplayName(connectionId: string): ResultAsync<string, TaskTrackerError> {
    return this.getConnection(connectionId)
      .andThen((conn) => this.getToken(conn).map((token) => ({ conn, token })))
      .asyncAndThen(({ conn, token }) => {
        const client = createProviderClient(conn.provider)
        return client.getCurrentUserDisplayName(conn, token)
      })
  }

  fetchBoards(
    connectionId: string,
    repoRoot?: string,
  ): ResultAsync<TrackerBoard[], TaskTrackerError> {
    return this.getConnection(connectionId)
      .andThen((conn) => this.getToken(conn).map((token) => ({ conn, token })))
      .asyncAndThen(({ conn, token }) =>
        this.resolveGitHubConnection(conn, repoRoot).andThen((resolved) => {
          const client = createProviderClient(resolved.provider)
          return client.fetchBoards(resolved, token)
        }),
      )
  }

  fetchStatuses(
    connectionId: string,
    boardId?: string,
    repoRoot?: string,
  ): ResultAsync<TrackerStatus[], TaskTrackerError> {
    return this.getConnection(connectionId)
      .andThen((conn) => this.getToken(conn).map((token) => ({ conn, token })))
      .asyncAndThen(({ conn, token }) =>
        this.resolveGitHubConnection(conn, repoRoot).andThen((resolved) => {
          const client = createProviderClient(resolved.provider)
          return client.fetchStatuses(resolved, token, boardId)
        }),
      )
  }

  fetchTasks(
    connectionId: string,
    params: { statuses?: string[]; assignedToMe?: boolean; boardId?: string },
    repoRoot?: string,
  ): ResultAsync<TrackerTask[], TaskTrackerError> {
    return this.getConnection(connectionId)
      .andThen((conn) => this.getToken(conn).map((token) => ({ conn, token })))
      .asyncAndThen(({ conn, token }) =>
        this.resolveGitHubConnection(conn, repoRoot).andThen((resolved) => {
          const client = createProviderClient(resolved.provider)
          return client.fetchTasks(resolved, token, params)
        }),
      )
  }

  getCurrentSprint(
    connectionId: string,
    boardId?: string,
    repoRoot?: string,
  ): ResultAsync<TrackerSprint | null, TaskTrackerError> {
    return this.getConnection(connectionId)
      .andThen((conn) => this.getToken(conn).map((token) => ({ conn, token })))
      .asyncAndThen(({ conn, token }) =>
        this.resolveGitHubConnection(conn, repoRoot).andThen((resolved) => {
          const client = createProviderClient(resolved.provider)
          return client.getCurrentSprint(resolved, token, boardId)
        }),
      )
  }

  fetchTaskComments(
    connectionId: string,
    taskKey: string,
    repoRoot?: string,
  ): ResultAsync<TrackerComment[], TaskTrackerError> {
    return this.getConnection(connectionId)
      .andThen((conn) => this.getToken(conn).map((token) => ({ conn, token })))
      .asyncAndThen(({ conn, token }) =>
        this.resolveGitHubConnection(conn, repoRoot).andThen((resolved) => {
          const client = createProviderClient(resolved.provider)
          return client.fetchTaskComments(resolved, token, taskKey)
        }),
      )
  }

  fetchTaskAttachments(
    connectionId: string,
    taskKey: string,
  ): ResultAsync<TrackerAttachment[], TaskTrackerError> {
    return this.getConnection(connectionId)
      .andThen((conn) => this.getToken(conn).map((token) => ({ conn, token })))
      .asyncAndThen(({ conn, token }) => {
        const client = createProviderClient(conn.provider)
        return client.fetchTaskAttachments(conn, token, taskKey)
      })
  }

  downloadAttachment(
    connectionId: string,
    url: string,
    filename: string,
  ): ResultAsync<string, TaskTrackerError> {
    const dlErr = (reason: string): TaskTrackerError => ({
      _tag: 'AttachmentDownloadFailed',
      filename,
      reason,
    })

    return this.getConnection(connectionId)
      .andThen((conn) => this.getToken(conn).map((token) => ({ conn, token })))
      .andThen(({ conn, token }) => {
        const connBase = conn.baseUrl.replace(/\/$/, '')
        if (!url.startsWith(connBase)) {
          return err(dlErr('URL does not match connection base URL'))
        }
        return ok({ conn, token })
      })
      .asyncAndThen(({ conn, token }) => this.downloadToTempDir(url, filename, conn, token, dlErr))
  }

  cleanupAttachmentDir(filePath: string): void {
    const tmpBase = os.tmpdir()
    const dir = join(filePath, '..')
    if (!dir.startsWith(tmpBase) || !basename(dir).startsWith('canopy-attachments-')) return
    try {
      rmSync(dir, { recursive: true, force: true })
    } catch {
      // Ignore cleanup errors
    }
  }
}
