#!/usr/bin/env node
import { spawnSync } from 'node:child_process'
import { mkdtempSync, readFileSync, rmSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { dirname, join, resolve } from 'node:path'
import { fileURLToPath, pathToFileURL } from 'node:url'

const __dirname = dirname(fileURLToPath(import.meta.url))
const projectRoot = resolve(__dirname, '..')

const profile = process.argv[2]
if (!profile || !['development', 'production'].includes(profile)) {
  console.error('Usage: node scripts/eas-update-publish.mjs <development|production>')
  process.exit(1)
}

process.env.NODE_ENV = profile
process.env.EXPO_PUBLIC_CHANNEL = profile

const configUrl = pathToFileURL(join(projectRoot, 'app.config.js')).href
const configModule = await import(configUrl)
const config = configModule.default ?? configModule
const updatesUrl = config?.expo?.updates?.url

if (!updatesUrl) {
  console.error(`Missing expo.updates.url in resolved config for profile "${profile}"`)
  process.exit(1)
}

console.log(`[eas-update] profile=${profile} channel=${profile}`)
console.log(`[eas-update] server=${updatesUrl}`)

const distDir = join(projectRoot, 'dist')
rmSync(distDir, { recursive: true, force: true })

console.log('[eas-update] running `expo export`...')
const exportRes = spawnSync('npx', ['expo', 'export', '--output-dir', 'dist'], {
  cwd: projectRoot,
  stdio: 'inherit',
  env: process.env,
})
if (exportRes.status !== 0) {
  console.error('[eas-update] expo export failed')
  process.exit(exportRes.status ?? 1)
}

const tmpDir = mkdtempSync(join(tmpdir(), 'canopy-update-'))
const tarPath = join(tmpDir, 'update.tar.gz')

console.log(`[eas-update] tarballing dist → ${tarPath}`)
const tarRes = spawnSync('tar', ['-czf', tarPath, '-C', distDir, '.'], {
  stdio: 'inherit',
})
if (tarRes.status !== 0) {
  console.error('[eas-update] tar failed')
  rmSync(tmpDir, { recursive: true, force: true })
  process.exit(tarRes.status ?? 1)
}

const uploadUrl = `${updatesUrl}/upload`
console.log(`[eas-update] uploading → ${uploadUrl}`)

const bytes = readFileSync(tarPath)
const form = new FormData()
form.set('file', new Blob([bytes], { type: 'application/gzip' }), 'update.tar.gz')

const res = await fetch(uploadUrl, {
  method: 'POST',
  headers: { 'x-eas-channel': profile },
  body: form,
})

rmSync(tmpDir, { recursive: true, force: true })

if (!res.ok) {
  const body = await res.text().catch(() => '')
  console.error(`[eas-update] upload failed: ${res.status} ${res.statusText}`)
  if (body) console.error(body)
  process.exit(1)
}

console.log('[eas-update] ✓ update uploaded successfully')
