// Local monotonic measurements only. Never compare these values with host clocks.
import { uniqueId } from './commands'
export type TimingStage = 'client_round_trip' | 'client_receive_apply' | 'render'
type TimingRecord = { sequence: number; host_id: string; resource_id: string; command_id: string; stage: TimingStage; duration_us: number; state: 'ok' | 'failed' }
type Summary = { stage: TimingStage; count: number; total_us: number; max_us: number; p95_upper_us: number }
const stages: TimingStage[] = ['client_round_trip', 'client_receive_apply', 'render']
const records: TimingRecord[] = [], histograms = new Map<TimingStage, { count: number; total: number; max: number; buckets: number[]; last: number }>()
let sequence = 0, dropped = 0, rejected = 0
let processId: string | undefined
const uuid = /^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/i
export function timingStart(): number { return performance.now() }
export function timing(host: string, resource: string, command: string | undefined, stage: TimingStage, started: number, ok = true): void {
  const now = timingStart(), us = Math.max(0, Math.ceil((now - started) * 1000))
  if (!stages.includes(stage) || !uuid.test(host) || !uuid.test(resource) || (command !== undefined && !uuid.test(command)) || !Number.isFinite(us) || started > now) { rejected++; return }
  const h = histograms.get(stage) ?? { count: 0, total: 0, max: 0, buckets: Array<number>(54).fill(0), last: -Infinity }
  h.count++; h.total = Math.min(Number.MAX_SAFE_INTEGER, h.total + us); h.max = Math.max(h.max, us)
  h.buckets[Math.min(53, us <= 1 ? 0 : Math.ceil(Math.log2(us)))]++
  histograms.set(stage, h)
  if (now - h.last < 100) { dropped++; return }
  h.last = now
  if (records.length === 256) { records.shift(); dropped++ }
  records.push({ sequence: ++sequence, host_id: host, resource_id: resource, command_id: command ?? '', stage, duration_us: us, state: ok ? 'ok' : 'failed' })
}
export function diagnostics(): { schema_version: number; process_instance_id: string; summaries: Summary[]; records: TimingRecord[]; dropped_records: number; rejected_keys: number } {
  return { schema_version: 1, process_instance_id: processId ??= uniqueId(), summaries: stages.map(stage => {
    const h = histograms.get(stage)
    if (!h) return { stage, count: 0, total_us: 0, max_us: 0, p95_upper_us: 0 }
    let seen = 0, p95 = 0
    for (let i = 0; i < h.buckets.length; i++) { seen += h.buckets[i]; if (seen >= Math.ceil(h.count * 0.95)) { p95 = 2 ** i; break } }
    return { stage, count: h.count, total_us: h.total, max_us: h.max, p95_upper_us: p95 }
  }), records: records.map(r => ({ ...r })), dropped_records: dropped, rejected_keys: rejected }
}
