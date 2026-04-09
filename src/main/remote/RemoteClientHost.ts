import http from 'node:http'
import fs from 'node:fs'
import path from 'node:path'
import { match } from 'ts-pattern'

const CSP_HEADER =
  "default-src 'self'; script-src 'self'; style-src 'self' 'unsafe-inline'; img-src 'self' data:; connect-src 'self' ws: wss:; font-src 'self' data:"

function contentTypeFor(ext: string): string {
  return match(ext.toLowerCase())
    .with('.html', () => 'text/html; charset=utf-8')
    .with('.js', '.mjs', () => 'text/javascript; charset=utf-8')
    .with('.css', () => 'text/css; charset=utf-8')
    .with('.svg', () => 'image/svg+xml')
    .with('.json', () => 'application/json; charset=utf-8')
    .with('.woff', () => 'font/woff')
    .with('.woff2', () => 'font/woff2')
    .with('.png', () => 'image/png')
    .with('.jpg', '.jpeg', () => 'image/jpeg')
    .with('.ico', () => 'image/x-icon')
    .with('.map', () => 'application/json; charset=utf-8')
    .otherwise(() => 'application/octet-stream')
}

/**
 * Serves the built remote-client SPA bundle (vite output) over HTTP so a
 * peer device on the LAN — phone, tablet, or another laptop — can load
 * `http://<lan-ip>:<port>/remote/`. The peer navigates there after scanning
 * the QR code; the SPA then opens a WebSocket to `/signaling`.
 *
 * Files are served from a single whitelisted root directory. Path traversal
 * (`..`) is rejected by resolving + checking the prefix. Static GET/HEAD only.
 *
 * NOTE on security scope: this host serves *the entire* renderer output dir
 * (including desktop chunks). The remote client entry happens to share
 * `assets/` with the desktop entries because vite emits all chunks into one
 * folder. This is acceptable for MVP because the renderer bundle does not
 * embed any secrets (API keys live in `PreferencesStore` and are accessed
 * via IPC). Splitting the remote client into a separate vite build is
 * tracked as future work.
 */
export class RemoteClientHost {
  private bundleRoot: string

  constructor(bundleRoot: string) {
    this.bundleRoot = path.resolve(bundleRoot)
  }

  /**
   * Whether the bundle root contains a built `remote.html`. Used by the
   * SignalingServer to short-circuit with a friendly error in dev mode where
   * the renderer is served from the Vite dev server, not from `out/renderer/`.
   */
  isAvailable(): boolean {
    try {
      return fs.statSync(path.join(this.bundleRoot, 'remote.html')).isFile()
    } catch {
      return false
    }
  }

  /**
   * Try to handle an incoming HTTP request. Returns `true` if it consumed the
   * request (sent a response), `false` if the path is outside `/remote/*` and
   * the caller should fall through to other routes.
   */
  handleRequest(req: http.IncomingMessage, res: http.ServerResponse): boolean {
    const url = new URL(req.url ?? '/', 'http://placeholder.invalid')
    const pathname = url.pathname

    // Anything outside /remote/* is not ours
    if (pathname !== '/remote' && !pathname.startsWith('/remote/')) return false

    if (req.method !== 'GET' && req.method !== 'HEAD') {
      res.writeHead(405, { Allow: 'GET, HEAD' })
      res.end()
      return true
    }

    // /remote or /remote/ → remote.html
    let relPath = pathname === '/remote' ? '' : pathname.slice('/remote/'.length)
    if (relPath === '' || relPath === '/') relPath = 'remote.html'

    // Sanitize: reject absolute or traversal
    if (relPath.startsWith('/') || relPath.includes('\0')) {
      res.writeHead(400)
      res.end()
      return true
    }

    const requested = path.normalize(path.join(this.bundleRoot, relPath))
    const rootWithSep = this.bundleRoot + path.sep
    if (requested !== this.bundleRoot && !requested.startsWith(rootWithSep)) {
      res.writeHead(403)
      res.end()
      return true
    }

    let stat: fs.Stats
    try {
      stat = fs.statSync(requested)
    } catch {
      res.writeHead(404, { 'Content-Type': 'text/plain; charset=utf-8' })
      res.end('Not found')
      return true
    }

    if (!stat.isFile()) {
      res.writeHead(404, { 'Content-Type': 'text/plain; charset=utf-8' })
      res.end('Not found')
      return true
    }

    const ext = path.extname(requested)
    // Stronger anti-cache headers for HTML specifically. Safari on iOS
    // has a long history of ignoring `no-store` alone for HTML documents
    // — it will happily serve the stale previous response on a refresh,
    // which is disastrous here because the stale HTML references JS
    // bundle filenames (content-hashed) that may not exist after a
    // rebuild. Piling on `no-cache, must-revalidate, max-age=0` plus
    // legacy `Pragma`/`Expires` plus an ETag that changes every
    // response finally forces Safari to re-fetch each time.
    const isHtml = ext.toLowerCase() === '.html'
    const headers: http.OutgoingHttpHeaders = {
      'Content-Type': contentTypeFor(ext),
      'Content-Length': stat.size,
      'Cache-Control': isHtml
        ? 'no-store, no-cache, must-revalidate, max-age=0, private'
        : 'no-store',
      'Content-Security-Policy': CSP_HEADER,
      'X-Content-Type-Options': 'nosniff',
      'Referrer-Policy': 'no-referrer',
      'X-Frame-Options': 'DENY',
    }
    if (isHtml) {
      headers['Pragma'] = 'no-cache'
      headers['Expires'] = '0'
      // Unique ETag per response defeats conditional-request cache hits
      // (`If-None-Match`). Date.now() is enough — we don't actually use
      // ETag for revalidation, we just need it to change every time.
      headers['ETag'] = `"${Date.now()}-${Math.random().toString(36).slice(2, 10)}"`
    }

    res.writeHead(200, headers)

    if (req.method === 'HEAD') {
      res.end()
      return true
    }

    const stream = fs.createReadStream(requested)
    stream.on('error', () => {
      // Connection may already be closed; just end quietly.
      res.end()
    })
    stream.pipe(res)
    return true
  }
}
