import { execFileSync } from 'node:child_process'
import { mkdtempSync, rmSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { resolve, dirname } from 'node:path'
import { fileURLToPath } from 'node:url'
const mobile = resolve(dirname(fileURLToPath(import.meta.url)), '..')
const output = mkdtempSync(resolve(tmpdir(), 'canopy-remote-tests-'))
try {
  execFileSync(process.env.TSC ?? resolve(mobile, 'node_modules/.bin/tsc'), ['--ignoreConfig', '--outDir', output, '--strict', '--skipLibCheck', '--target', 'ES2022', '--moduleResolution', 'node16', '--module', 'Node16', ...['schema','protobuf','persistence','catalog','outbox','transport','fragments','projection','uploads','terminal','chat','files','artifact-preview','chat-commands'].map(f => `src/lib/remote/grpc/${f}.ts`)], { cwd: mobile, stdio: 'inherit' })
  execFileSync(process.execPath, ['--test', 'tests/remote.test.cjs', 'tests/catalog.test.cjs', 'tests/metrics.test.cjs'], { cwd: mobile, env: { ...process.env, CANOPY_TEST_BUILD: output }, stdio: 'inherit' })
} finally { rmSync(output, { recursive: true, force: true }) }
