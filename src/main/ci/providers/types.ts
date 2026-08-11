import type { ResultAsync } from 'neverthrow'
import type { CiError } from '../errors'
import type {
  CiJobStatus,
  CiParameterSet,
  CiRef,
  CiRun,
  CiRunActivity,
  CiRunTriggerResult,
  CiTriggerRequest,
} from '../types'

/** Main-process-only provider boundary. Instances are bound to validated config and credentials. */
export interface CiProviderAdapter {
  status(ref: CiRef): ResultAsync<CiJobStatus[], CiError>
  refs(jobId: string): ResultAsync<CiRef[], CiError>
  exactRef(jobId: string, name: string): ResultAsync<CiRef, CiError>
  parameters(jobId: string, ref: CiRef): ResultAsync<CiParameterSet, CiError>
  trigger(request: CiTriggerRequest): ResultAsync<CiRunTriggerResult, CiError>
  run(runId: string): ResultAsync<CiRun, CiError>
  /** `branch` scopes the provider's own query — the result caps are applied before we see them. */
  activity(branch?: string): ResultAsync<CiRunActivity, CiError>
}
