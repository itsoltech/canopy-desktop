import { safeStorage } from 'electron'
import { randomUUID } from 'crypto'
import type { Database as BetterSqlite3Database } from 'better-sqlite3'
import type { Database } from './Database'

interface CredentialRow {
  id: string
  domain: string
  username: string
  password_enc: string
  title: string
  created_at: string
  updated_at: string
}

export interface Credential {
  id: string
  domain: string
  username: string
  password: string
  title: string
  createdAt: string
  updatedAt: string
}

export interface CredentialMasked {
  id: string
  domain: string
  username: string
  title: string
  createdAt: string
  updatedAt: string
}

export class CredentialStore {
  constructor(private database: Database) {}

  private get db(): BetterSqlite3Database {
    return this.database.db
  }

  private encrypt(plaintext: string): string {
    if (safeStorage.isEncryptionAvailable()) {
      return safeStorage.encryptString(plaintext).toString('base64')
    }
    // Fallback: base64 only (no OS keychain available, e.g. Linux without keyring)
    console.warn(
      '[CredentialStore] safeStorage encryption unavailable — credentials stored without OS-level encryption. Configure a system keyring for secure storage.',
    )
    return Buffer.from(plaintext).toString('base64')
  }

  private decrypt(stored: string): string {
    if (safeStorage.isEncryptionAvailable()) {
      try {
        return safeStorage.decryptString(Buffer.from(stored, 'base64'))
      } catch {
        // Fallback: plain base64 (stored before encryption was available)
        return Buffer.from(stored, 'base64').toString()
      }
    }
    return Buffer.from(stored, 'base64').toString()
  }

  /** Returns credentials WITH decrypted passwords — internal use only, never expose via IPC directly */
  getForDomain(domain: string): Credential[] {
    const rows = this.db
      .prepare('SELECT * FROM credentials WHERE domain = ?')
      .all(domain) as CredentialRow[]
    return rows.map((row) => ({
      id: row.id,
      domain: row.domain,
      username: row.username,
      password: this.decrypt(row.password_enc),
      title: row.title,
      createdAt: row.created_at,
      updatedAt: row.updated_at,
    }))
  }

  /** Returns credentials WITHOUT passwords — safe for IPC */
  getForDomainMasked(domain: string): CredentialMasked[] {
    const rows = this.db
      .prepare(
        'SELECT id, domain, username, title, created_at, updated_at FROM credentials WHERE domain = ?',
      )
      .all(domain) as Omit<CredentialRow, 'password_enc'>[]
    return rows.map((row) => ({
      id: row.id,
      domain: row.domain,
      username: row.username,
      title: row.title,
      createdAt: row.created_at,
      updatedAt: row.updated_at,
    }))
  }

  save(domain: string, username: string, password: string, title: string = ''): void {
    const id = randomUUID()
    const enc = this.encrypt(password)
    this.db
      .prepare(
        `INSERT INTO credentials (id, domain, username, password_enc, title, updated_at)
         VALUES (?, ?, ?, ?, ?, datetime('now'))
         ON CONFLICT(domain, username) DO UPDATE SET
           password_enc = excluded.password_enc,
           title = excluded.title,
           updated_at = datetime('now')`,
      )
      .run(id, domain, username, enc, title)
  }

  delete(id: string): void {
    this.db.prepare('DELETE FROM credentials WHERE id = ?').run(id)
  }

  /** List all credentials with passwords masked (for Settings UI) */
  getAll(): CredentialMasked[] {
    const rows = this.db
      .prepare(
        'SELECT id, domain, username, title, created_at, updated_at FROM credentials ORDER BY domain',
      )
      .all() as Omit<CredentialRow, 'password_enc'>[]
    return rows.map((row) => ({
      id: row.id,
      domain: row.domain,
      username: row.username,
      title: row.title,
      createdAt: row.created_at,
      updatedAt: row.updated_at,
    }))
  }

  /**
   * Returns every credential with decrypted password. Main-process-only;
   * never expose via IPC. Used for settings export.
   */
  listInternalDecrypted(): Credential[] {
    const rows = this.db
      .prepare('SELECT * FROM credentials ORDER BY domain, username')
      .all() as CredentialRow[]
    return rows.map((row) => ({
      id: row.id,
      domain: row.domain,
      username: row.username,
      password: this.decrypt(row.password_enc),
      title: row.title,
      createdAt: row.created_at,
      updatedAt: row.updated_at,
    }))
  }

  /**
   * Upsert credentials from an import file, keyed by (domain, username).
   * Existing rows are updated in place (id preserved); new rows get a fresh
   * UUID. Re-encrypts passwords with this machine's safeStorage. Does not
   * open a transaction — the caller wraps the full import in one outer
   * transaction.
   */
  upsertForImport(
    creds: {
      domain: string
      username: string
      password: string
      title?: string
    }[],
  ): number {
    const findStmt = this.db.prepare('SELECT id FROM credentials WHERE domain = ? AND username = ?')
    const updateStmt = this.db.prepare(
      `UPDATE credentials
         SET password_enc = ?, title = ?, updated_at = datetime('now')
         WHERE id = ?`,
    )
    const insertStmt = this.db.prepare(
      `INSERT INTO credentials (id, domain, username, password_enc, title, updated_at)
       VALUES (?, ?, ?, ?, ?, datetime('now'))`,
    )

    let count = 0
    for (const c of creds) {
      if (!c.domain || !c.username) continue
      const enc = this.encrypt(c.password)
      const existing = findStmt.get(c.domain, c.username) as { id: string } | undefined
      if (existing) {
        updateStmt.run(enc, c.title ?? '', existing.id)
      } else {
        insertStmt.run(randomUUID(), c.domain, c.username, enc, c.title ?? '')
      }
      count++
    }
    return count
  }

  /** Get single credential with decrypted password (for autofill) */
  getById(id: string): Credential | null {
    const row = this.db.prepare('SELECT * FROM credentials WHERE id = ?').get(id) as
      | CredentialRow
      | undefined
    if (!row) return null
    return {
      id: row.id,
      domain: row.domain,
      username: row.username,
      password: this.decrypt(row.password_enc),
      title: row.title,
      createdAt: row.created_at,
      updatedAt: row.updated_at,
    }
  }
}
