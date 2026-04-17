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
    // Use -i (interactive) instead of -li (login) to avoid running
    // expensive rc file operations (update checks, prompts, etc.) while still
    // capturing PATH modifications from .bashrc/.zshrc
    execFile(
      shell,
      ['-i', '-c', 'env -0'],
      { timeout: 10000, maxBuffer: 1024 * 1024 },
      (err, stdout) => {
        if (err) {
          // Fall back to process.env if login shell fails
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
