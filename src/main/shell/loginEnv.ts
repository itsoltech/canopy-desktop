import { execFile } from 'child_process'
import os from 'os'

let cachedEnv: Record<string, string> | null = null

/**
 * Spawns the user's login shell to capture the full environment
 * (including PATH modifications from .zshrc, .bashrc, config.fish, etc.)
 * Caches the result so it's only resolved once.
 */
export async function resolveLoginEnv(): Promise<Record<string, string>> {
  if (cachedEnv) return cachedEnv

  if (os.platform() === 'win32') {
    cachedEnv = { ...process.env } as Record<string, string>
    return cachedEnv
  }

  const shell = process.env.SHELL || '/bin/bash'

  return new Promise((resolve) => {
    // Both flags are needed:
    //   -i sources .zshrc/.bashrc (user PATH exports, aliases)
    //   -l sources .zprofile/.zlogin and /etc/zprofile, which on macOS runs
    //      /usr/libexec/path_helper — required for /opt/homebrew/bin etc.
    //      to appear in PATH when Canopy is launched from Finder.
    execFile(
      shell,
      ['-i', '-l', '-c', 'env -0'],
      { timeout: 10000, maxBuffer: 1024 * 1024 },
      (err, stdout) => {
        if (err) {
          console.warn(`[loginEnv] failed to source ${shell}: ${err.message}`)
          cachedEnv = { ...process.env } as Record<string, string>
          resolve(cachedEnv)
          return
        }

        const env: Record<string, string> = {}
        const entries = stdout.split('\0')
        for (const entry of entries) {
          if (!entry) continue
          const idx = entry.indexOf('=')
          if (idx > 0) {
            env[entry.substring(0, idx)] = entry.substring(idx + 1)
          }
        }

        // Ensure we got a valid env (at least PATH should exist)
        if (!env.PATH) {
          cachedEnv = { ...process.env } as Record<string, string>
          resolve(cachedEnv)
          return
        }

        cachedEnv = env
        resolve(cachedEnv)
      },
    )
  })
}

export function getLoginEnv(): Record<string, string> | null {
  return cachedEnv
}
