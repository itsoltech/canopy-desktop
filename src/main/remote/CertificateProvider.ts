import fs from 'node:fs'
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
 * Phase 13 creates the infrastructure. The `SignalingServer` still listens
 * on plain HTTP by default — HTTPS is only used when the preference toggle
 * is on.
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

    // Reuse cached cert if the LAN IP hasn't changed
    if (fs.existsSync(certPath) && fs.existsSync(keyPath) && fs.existsSync(metaPath)) {
      try {
        const meta = JSON.parse(fs.readFileSync(metaPath, 'utf-8'))
        if (meta.lanIp === lanIp) {
          return {
            cert: fs.readFileSync(certPath, 'utf-8'),
            key: fs.readFileSync(keyPath, 'utf-8'),
          }
        }
      } catch {
        // Corrupted cache — regenerate
      }
    }

    const notAfterDate = new Date()
    notAfterDate.setFullYear(notAfterDate.getFullYear() + 1)

    const attrs = [{ name: 'commonName', value: 'canopy-remote' }]
    const pems = await selfsigned.generate(attrs, {
      notAfterDate,
      keySize: 2048,
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

    fs.mkdirSync(this.certDir, { recursive: true })
    fs.writeFileSync(certPath, pems.cert, 'utf-8')
    fs.writeFileSync(keyPath, pems.private, 'utf-8')
    fs.writeFileSync(
      metaPath,
      JSON.stringify({ lanIp, createdAt: new Date().toISOString() }),
      'utf-8',
    )

    return { cert: pems.cert, key: pems.private }
  }
}
