import { promises as fs } from 'node:fs'
import path from 'node:path'
import selfsigned from 'selfsigned'

/**
 * Lazy generator + file-system cache for a self-signed TLS certificate used
 * by the signaling server when `remote.useHttps` is enabled in preferences.
 *
 * The cert is written to `<userData>/remote/cert.pem` + `key.pem` and reused
 * across app restarts. It is regenerated whenever the LAN IP changes (the
 * subject-alt-name must match the IP the peer connects to, otherwise remote
 * browsers show a hostname-mismatch warning on top of the self-signed
 * warning).
 *
 * Phase 13 created this infrastructure, but it is NOT wired up: nothing
 * constructs a `CertificateProvider`, no `remote.useHttps` preference exists,
 * and `SignalingServer` unconditionally calls `http.createServer`. The
 * signaling channel — including the one-shot pairing token and the
 * trusted-device id — is therefore always plaintext on the LAN. That is the
 * current threat model until this is either wired in or deleted.
 */
export class CertificateProvider {
  private certDir: string

  constructor(userDataPath: string) {
    this.certDir = path.join(userDataPath, 'remote')
  }

  async getCertificate(lanIp: string): Promise<{ cert: string; key: string }> {
    const certPath = path.join(this.certDir, 'cert.pem')
    const keyPath = path.join(this.certDir, 'key.pem')
    const metaPath = path.join(this.certDir, 'cert-meta.json')

    // Reuse cached cert if the LAN IP hasn't changed.
    // Use async fs throughout so cert I/O never blocks the main-process event
    // loop — writing the 2048-bit key is CPU-light but sync writes stall UI.
    const cached = await this.readCached(certPath, keyPath, metaPath, lanIp)
    if (cached) return cached

    const notAfterDate = new Date()
    notAfterDate.setFullYear(notAfterDate.getFullYear() + 1)

    const attrs = [{ name: 'commonName', value: 'canopy-remote' }]
    const pems = await selfsigned.generate(attrs, {
      notAfterDate,
      keySize: 2048,
      // selfsigned defaults to SHA-1 when `algorithm` is omitted; pin SHA-256
      // so the signaling TLS cert is not signed with a deprecated hash.
      algorithm: 'sha256',
      extensions: [
        {
          name: 'subjectAltName',
          altNames: [
            { type: 2, value: 'localhost' },
            { type: 7, ip: '127.0.0.1' },
            { type: 7, ip: lanIp },
          ],
        },
      ],
    })

    // Tighten directory and file permissions so the private key is not
    // world-readable. The cert itself is public so 0o644 is fine, but the
    // key MUST be owner-only (0o600) — without this mode, the default
    // umask makes it world-readable on any multi-user system.
    await fs.mkdir(this.certDir, { recursive: true, mode: 0o700 })
    await fs.writeFile(certPath, pems.cert, { encoding: 'utf-8', mode: 0o644 })
    await fs.writeFile(keyPath, pems.private, { encoding: 'utf-8', mode: 0o600 })
    await fs.writeFile(metaPath, JSON.stringify({ lanIp, createdAt: new Date().toISOString() }), {
      encoding: 'utf-8',
      mode: 0o600,
    })

    return { cert: pems.cert, key: pems.private }
  }

  private async readCached(
    certPath: string,
    keyPath: string,
    metaPath: string,
    lanIp: string,
  ): Promise<{ cert: string; key: string } | null> {
    try {
      const metaRaw = await fs.readFile(metaPath, 'utf-8')
      const meta = JSON.parse(metaRaw)
      if (meta.lanIp !== lanIp) return null
      const [cert, key] = await Promise.all([
        fs.readFile(certPath, 'utf-8'),
        fs.readFile(keyPath, 'utf-8'),
      ])
      return { cert, key }
    } catch {
      // Missing or corrupted cache — caller regenerates.
      return null
    }
  }
}
