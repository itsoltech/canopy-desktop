import { match } from 'ts-pattern'

export type CiError =
  | { _tag: 'CiNotConfigured' }
  | { _tag: 'CiAuthMissing'; baseUrl: string }
  | { _tag: 'CiApiError'; status: number; message: string }

export function ciErrorMessage(error: CiError): string {
  return match(error)
    .with({ _tag: 'CiNotConfigured' }, () => 'No CI configured for this repository')
    .with(
      { _tag: 'CiAuthMissing' },
      (e) => `No TeamCity token stored for ${e.baseUrl} — connect it in Settings`,
    )
    .with({ _tag: 'CiApiError' }, (e) =>
      e.status > 0 ? `TeamCity API error ${e.status}: ${e.message}` : `TeamCity: ${e.message}`,
    )
    .exhaustive()
}
