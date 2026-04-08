import { match } from 'ts-pattern'

export type RunConfigError =
  | { _tag: 'RunConfigNotFound'; path: string }
  | { _tag: 'RunConfigParseError'; path: string; reason: string }
  | { _tag: 'RunConfigWriteError'; path: string; reason: string }
  | { _tag: 'RunConfigValidationError'; name: string; reason: string }
  | { _tag: 'RunConfigExecutionError'; name: string; reason: string }

export function runConfigErrorMessage(error: RunConfigError): string {
  return match(error)
    .with({ _tag: 'RunConfigNotFound' }, (e) => `Run config not found: ${e.path}`)
    .with({ _tag: 'RunConfigParseError' }, (e) => `Invalid run config at ${e.path}: ${e.reason}`)
    .with(
      { _tag: 'RunConfigWriteError' },
      (e) => `Failed to write run config at ${e.path}: ${e.reason}`,
    )
    .with(
      { _tag: 'RunConfigValidationError' },
      (e) => `Invalid configuration "${e.name}": ${e.reason}`,
    )
    .with({ _tag: 'RunConfigExecutionError' }, (e) => `Failed to execute "${e.name}": ${e.reason}`)
    .exhaustive()
}
