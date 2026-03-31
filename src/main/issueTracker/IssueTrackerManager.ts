import type { PreferencesStore } from '../db/PreferencesStore'
import { createProviderClient } from './providers'
import type {
  IssueTrackerConnection,
  TrackerBoard,
  TrackerIssue,
  TrackerSprint,
  TrackerStatus,
} from './types'

const CONNECTIONS_PREF_KEY = 'issueTracker.connections'

export class IssueTrackerManager {
  constructor(private preferencesStore: PreferencesStore) {}

  getConnections(): IssueTrackerConnection[] {
    const raw = this.preferencesStore.get(CONNECTIONS_PREF_KEY)
    if (!raw) return []
    try {
      return JSON.parse(raw) as IssueTrackerConnection[]
    } catch {
      return []
    }
  }

  private saveConnections(connections: IssueTrackerConnection[]): void {
    this.preferencesStore.set(CONNECTIONS_PREF_KEY, JSON.stringify(connections))
  }

  private getConnection(connectionId: string): IssueTrackerConnection {
    const conn = this.getConnections().find((c) => c.id === connectionId)
    if (!conn) throw new Error(`Connection not found: ${connectionId}`)
    return conn
  }

  private getToken(connection: IssueTrackerConnection): string {
    const token = this.preferencesStore.get(connection.authPrefKey)
    if (!token) throw new Error(`No auth token for connection: ${connection.name}`)
    return token
  }

  addConnection(
    connection: Omit<IssueTrackerConnection, 'id' | 'authPrefKey'>,
    token: string,
  ): IssueTrackerConnection {
    const id = crypto.randomUUID()
    const authPrefKey = `issueTracker.token.${id}`

    const newConn: IssueTrackerConnection = {
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

  removeConnection(connectionId: string): void {
    const connections = this.getConnections()
    const conn = connections.find((c) => c.id === connectionId)
    if (conn) {
      this.preferencesStore.delete(conn.authPrefKey)
    }
    this.saveConnections(connections.filter((c) => c.id !== connectionId))
  }

  async testConnection(connectionId: string): Promise<boolean> {
    const conn = this.getConnection(connectionId)
    const token = this.getToken(conn)
    const client = createProviderClient(conn.provider)
    return client.testConnection(conn, token)
  }

  async testNewConnection(
    connection: Omit<IssueTrackerConnection, 'id' | 'authPrefKey'>,
    token: string,
  ): Promise<boolean> {
    const tempConn: IssueTrackerConnection = {
      ...connection,
      id: 'temp',
      authPrefKey: 'temp',
    }
    const client = createProviderClient(connection.provider)
    return client.testConnection(tempConn, token)
  }

  async fetchBoardsForNew(
    connection: Omit<IssueTrackerConnection, 'id' | 'authPrefKey'>,
    token: string,
  ): Promise<TrackerBoard[]> {
    const tempConn: IssueTrackerConnection = {
      ...connection,
      id: 'temp',
      authPrefKey: 'temp',
    }
    const client = createProviderClient(connection.provider)
    return client.fetchBoards(tempConn, token)
  }

  async findIssueByKey(issueKey: string): Promise<TrackerIssue | null> {
    const connections = this.getConnections()
    for (const conn of connections) {
      try {
        const token = this.getToken(conn)
        const client = createProviderClient(conn.provider)
        const issue = await client.fetchIssueByKey(conn, token, issueKey)
        if (issue) return issue
      } catch {
        // try next connection
      }
    }
    return null
  }

  async getCurrentUserDisplayName(connectionId: string): Promise<string> {
    const conn = this.getConnection(connectionId)
    const token = this.getToken(conn)
    const client = createProviderClient(conn.provider)
    return client.getCurrentUserDisplayName(conn, token)
  }

  async fetchBoards(connectionId: string): Promise<TrackerBoard[]> {
    const conn = this.getConnection(connectionId)
    const token = this.getToken(conn)
    const client = createProviderClient(conn.provider)
    return client.fetchBoards(conn, token)
  }

  async fetchStatuses(connectionId: string, boardId?: string): Promise<TrackerStatus[]> {
    const conn = this.getConnection(connectionId)
    const token = this.getToken(conn)
    const client = createProviderClient(conn.provider)
    return client.fetchStatuses(conn, token, boardId)
  }

  async fetchIssues(
    connectionId: string,
    params: { statuses?: string[]; assignedToMe?: boolean; boardId?: string },
  ): Promise<TrackerIssue[]> {
    const conn = this.getConnection(connectionId)
    const token = this.getToken(conn)
    const client = createProviderClient(conn.provider)
    return client.fetchIssues(conn, token, params)
  }

  async getCurrentSprint(connectionId: string, boardId?: string): Promise<TrackerSprint | null> {
    const conn = this.getConnection(connectionId)
    const token = this.getToken(conn)
    const client = createProviderClient(conn.provider)
    return client.getCurrentSprint(conn, token, boardId)
  }
}
