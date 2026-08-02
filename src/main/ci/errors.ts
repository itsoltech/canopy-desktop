import { match } from 'ts-pattern'

export type CiError =
  | { _tag: 'CiNotConfigured' }
  // A ci block EXISTS but cannot be used — kept distinct from "not configured":
  // the surfaces that hit this only exist because the block is there, so a
  // "set up CI" message would send the user hunting for a setting they have.
  | { _tag: 'CiConfigInvalid'; reason: string }
  | { _tag: 'CiAuthMissing'; baseUrl: string }
  | { _tag: 'CiApiError'; status: number; message: string }

export function ciErrorMessage(error: CiError): string {
  return match(error)
    .with({ _tag: 'CiNotConfigured' }, () => 'No CI configured for this repository')
    .with(
      { _tag: 'CiConfigInvalid' },
      // Reason FIRST: the sidebar renders this in a truncated column, and a
      // constant prefix would eat exactly the width the reason needs. Every
      // surface (sidebar, configurator) renders this one string — nothing may
      // re-parse or strip it.
      (e) => `${e.reason} — invalid ci block in .canopy/config.json`,
    )
    .with(
      { _tag: 'CiAuthMissing' },
      (e) => `No TeamCity token stored for ${e.baseUrl} — connect it in Settings`,
    )
    .with({ _tag: 'CiApiError' }, (e) =>
      e.status > 0 ? `TeamCity API error ${e.status}: ${e.message}` : `TeamCity: ${e.message}`,
    )
    .exhaustive()
}
