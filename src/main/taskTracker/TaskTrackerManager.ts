import { join, basename } from 'path'
import { mkdirSync, createWriteStream, rmSync } from 'fs'
import os from 'os'
import { randomUUID } from 'crypto'
import { pipeline } from 'stream/promises'
import { Readable } from 'stream'
import { ok, err, type Result, type ResultAsync } from 'neverthrow'
import type { PreferencesStore } from '../db/PreferencesStore'
import type { TaskTrackerError } from './errors'
import { createProviderClient } from './providers'
import type {
  TaskTrackerConnection,
  TrackerAttachment,
  TrackerBoard,
  TrackerComment,
  TrackerTask,
  TrackerSprint,
  TrackerStatus,
} from './types'

const CONNECTIONS_PREF_KEY = 'taskTracker.connections'

export class TaskTrackerManager {
  constructor(private preferencesStore: PreferencesStore) {}

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
      const tokenResult = this.getToken(conn)
      if (tokenResult.isErr()) continue
      const client = createProviderClient(conn.provider)
      const result = await client.fetchTaskByKey(conn, tokenResult.value, taskKey)
      if (result.isOk() && result.value) return result.value
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

  fetchBoards(connectionId: string): ResultAsync<TrackerBoard[], TaskTrackerError> {
    return this.getConnection(connectionId)
      .andThen((conn) => this.getToken(conn).map((token) => ({ conn, token })))
      .asyncAndThen(({ conn, token }) => {
        const client = createProviderClient(conn.provider)
        return client.fetchBoards(conn, token)
      })
  }

  fetchStatuses(
    connectionId: string,
    boardId?: string,
  ): ResultAsync<TrackerStatus[], TaskTrackerError> {
    return this.getConnection(connectionId)
      .andThen((conn) => this.getToken(conn).map((token) => ({ conn, token })))
      .asyncAndThen(({ conn, token }) => {
        const client = createProviderClient(conn.provider)
        return client.fetchStatuses(conn, token, boardId)
      })
  }

  fetchTasks(
    connectionId: string,
    params: { statuses?: string[]; assignedToMe?: boolean; boardId?: string },
  ): ResultAsync<TrackerTask[], TaskTrackerError> {
    return this.getConnection(connectionId)
      .andThen((conn) => this.getToken(conn).map((token) => ({ conn, token })))
      .asyncAndThen(({ conn, token }) => {
        const client = createProviderClient(conn.provider)
        return client.fetchTasks(conn, token, params)
      })
  }

  getCurrentSprint(
    connectionId: string,
    boardId?: string,
  ): ResultAsync<TrackerSprint | null, TaskTrackerError> {
    return this.getConnection(connectionId)
      .andThen((conn) => this.getToken(conn).map((token) => ({ conn, token })))
      .asyncAndThen(({ conn, token }) => {
        const client = createProviderClient(conn.provider)
        return client.getCurrentSprint(conn, token, boardId)
      })
  }

  fetchTaskComments(
    connectionId: string,
    taskKey: string,
  ): ResultAsync<TrackerComment[], TaskTrackerError> {
    return this.getConnection(connectionId)
      .andThen((conn) => this.getToken(conn).map((token) => ({ conn, token })))
      .asyncAndThen(({ conn, token }) => {
        const client = createProviderClient(conn.provider)
        return client.fetchTaskComments(conn, token, taskKey)
      })
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

  async downloadAttachment(connectionId: string, url: string, filename: string): Promise<string> {
    const connResult = this.getConnection(connectionId)
    const tokenResult = connResult.andThen((conn) => this.getToken(conn).map((t) => ({ conn, t })))
    if (tokenResult.isErr()) throw new Error('Connection or auth error')
    const { conn, t: token } = tokenResult.value

    const connBase = conn.baseUrl.replace(/\/$/, '')
    if (!url.startsWith(connBase)) {
      throw new Error('Attachment URL does not match connection base URL')
    }

    const dir = join(os.tmpdir(), `canopy-attachments-${randomUUID()}`)
    mkdirSync(dir, { recursive: true })

    const safeName = basename(filename.replace(/[/\\]/g, '_'))
    const filePath = join(dir, safeName)

    const headers: Record<string, string> = {
      Authorization: conn.username
        ? `Basic ${Buffer.from(`${conn.username}:${token}`).toString('base64')}`
        : `Bearer ${token}`,
    }

    const res = await fetch(url, { headers, signal: AbortSignal.timeout(60_000) })
    if (!res.ok) throw new Error(`Download failed: ${res.status}`)
    if (!res.body) throw new Error('Empty response body')

    const nodeStream = Readable.fromWeb(res.body as import('stream/web').ReadableStream)
    await pipeline(nodeStream, createWriteStream(filePath))

    return filePath
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
