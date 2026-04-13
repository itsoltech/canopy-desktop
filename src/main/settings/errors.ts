import { match } from 'ts-pattern'

export type SettingsExportError =
  | { _tag: 'ExportReadError'; reason: string }
  | { _tag: 'ExportWriteError'; reason: string }
  | { _tag: 'ImportReadError'; reason: string }
  | { _tag: 'ImportParseError'; reason: string }
  | { _tag: 'ImportVersionMismatch'; found: number; expected: number }
  | { _tag: 'ImportValidationError'; reason: string }
  | { _tag: 'ImportWriteError'; reason: string }

export function settingsExportErrorMessage(error: SettingsExportError): string {
  return match(error)
    .with({ _tag: 'ExportReadError' }, (e) => `Failed to read settings: ${e.reason}`)
    .with({ _tag: 'ExportWriteError' }, (e) => `Failed to write export file: ${e.reason}`)
    .with({ _tag: 'ImportReadError' }, (e) => `Failed to read import file: ${e.reason}`)
    .with({ _tag: 'ImportParseError' }, (e) => `Import file is not valid JSON: ${e.reason}`)
    .with(
      { _tag: 'ImportVersionMismatch' },
      (e) =>
        `Import file version ${e.found} is not supported (expected ${e.expected}). ` +
        `Export from a newer or older version of Canopy.`,
    )
    .with({ _tag: 'ImportValidationError' }, (e) => `Import file is invalid: ${e.reason}`)
    .with({ _tag: 'ImportWriteError' }, (e) => `Failed to apply import: ${e.reason}`)
    .exhaustive()
}
