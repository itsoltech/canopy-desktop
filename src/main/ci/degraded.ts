import type { CiError } from './errors'

const DEGRADED_CAUSES = Symbol('ci.degradedCauses')

type DegradedValue = object & { [DEGRADED_CAUSES]?: readonly CiError[] }

/** Attach main-process-only structured causes without adding enumerable IPC payload fields. */
export function withCiDegradedCauses<T extends object>(value: T, causes: readonly CiError[]): T {
  Object.defineProperty(value, DEGRADED_CAUSES, { value: causes })
  return value
}

export function ciDegradedCauses(value: unknown): readonly CiError[] | undefined {
  return value && typeof value === 'object' ? (value as DegradedValue)[DEGRADED_CAUSES] : undefined
}
