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
