import { ResultAsync } from 'neverthrow'

export { ok, err, okAsync, errAsync, ResultAsync } from 'neverthrow'
export type { Result, ResultAsync as ResultAsyncType } from 'neverthrow'

export function fromExternalCall<T, E>(
  promise: Promise<T>,
  mapErr: (e: unknown) => E,
): ResultAsync<T, E> {
  return ResultAsync.fromPromise(promise, mapErr)
}

export function errorMessage(e: unknown): string {
  return e instanceof Error ? e.message : String(e)
}
