import { match } from 'ts-pattern'

export type CiError =
  | { _tag: 'CiNotConfigured' }
  // A ci block EXISTS but cannot be used — kept distinct from "not configured":
  // the surfaces that hit this only exist because the block is there, so a
  // "set up CI" message would send the user hunting for a setting they have.
  // `scope` records WHERE the problem is: 'file' = the whole .canopy/config.json
  // cannot be read (bad JSON, unsupported version, a legacy tracker provider —
  // saveConfig refuses then), 'block' = only the ci block's shape is rejected
  // (re-saving replaces it).
  | { _tag: 'CiConfigInvalid'; scope: 'file' | 'block'; reason: string }
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
      // re-parse or strip it. The suffix must not blame the ci block for a
      // file-level failure: most file reasons (bad JSON, unsupported version, a
      // legacy tracker provider) say nothing about `ci`.
      (e) =>
        e.scope === 'file'
          ? `${e.reason} — .canopy/config.json cannot be read`
          : `${e.reason} — invalid ci block in .canopy/config.json`,
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
