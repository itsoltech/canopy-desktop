import { match } from 'ts-pattern'

export type CiError =
  | { _tag: 'CiNotConfigured' }
  // A ci block EXISTS but cannot be used — kept distinct from "not configured":
  // the surfaces that hit this only exist because the block is there, so a
  // "set up CI" message would send the user hunting for a setting they have.
  // `scope` records WHERE the problem is: 'file' = the whole .canopy/config.json
  // cannot be used (bad JSON, or a field the loader rejects — an unsupported
  // version, a legacy tracker provider; the last two parse fine, which is why
  // this is "cannot be used" and not "cannot be read"). saveConfig refuses then.
  // 'block' = only the ci block's shape is rejected (re-saving replaces it).
  | {
      _tag: 'CiConfigInvalid'
      scope: 'file' | 'block'
      reason: string
      provider?: 'teamcity' | 'github-actions'
    }
  // A local filesystem failure while saving the repo config — never TeamCity's
  // fault, so it must not wear the "TeamCity:" prefix CiApiError renders with.
  | { _tag: 'CiConfigUnwritable'; reason: string }
  | { _tag: 'CiAuthMissing'; baseUrl: string; provider?: 'teamcity' | 'github-actions' }
  | { _tag: 'CiRepositoryMismatch'; expected: string; actual: string }
  | { _tag: 'CiWorkflowSchemaInvalid'; reason: string }
  | { _tag: 'CiWorkflowSchemaChanged' }
  | { _tag: 'CiRefChanged' }
  | { _tag: 'CiDispatchCancelled' }
  | { _tag: 'CiDispatchAmbiguous'; workflowUrl: string }
  | { _tag: 'CiRateLimited'; resetAt: number | undefined }
  | {
      _tag: 'CiApiError'
      status: number
      message: string
      provider?: 'teamcity' | 'github-actions'
    }

export function ciErrorMessage(error: CiError): string {
  return (
    match(error)
      .with({ _tag: 'CiNotConfigured' }, () => 'No CI configured for this repository')
      .with(
        { _tag: 'CiConfigInvalid' },
        // Reason FIRST: the sidebar renders this in a truncated column, and a
        // constant prefix would eat exactly the width the reason needs. Every
        // surface (sidebar, configurator) renders this one string — nothing may
        // re-parse or strip it. The suffix must not blame the ci block for a
        // file-level failure: most file reasons (bad JSON, unsupported version, a
        // legacy tracker provider) say nothing about `ci`.
        // "cannot be USED", not "read": two of the file-scope reasons (unsupported
        // version, a legacy tracker provider) parse fine — a specific field was
        // rejected, so "cannot be read" would send the user hunting for corruption.
        (e) =>
          e.scope === 'file'
            ? `${e.reason} — .canopy/config.json cannot be used`
            : `${e.reason} — invalid ci block in .canopy/config.json`,
      )
      // "update", not "write": the reason may itself be a READ failure (the write
      // is gated on reading the existing file first), and "could not write —
      // could not be read" would scan as a contradiction.
      .with(
        { _tag: 'CiConfigUnwritable' },
        (e) => `Could not update .canopy/config.json — ${e.reason}`,
      )
      .with(
        { _tag: 'CiAuthMissing' },
        (e) =>
          `No ${e.provider === 'github-actions' ? 'GitHub' : 'TeamCity'} token stored for ${e.baseUrl} — connect it in Settings`,
      )
      .with(
        { _tag: 'CiRepositoryMismatch' },
        (e) =>
          `Configured GitHub repository ${e.expected} does not match this workspace (${e.actual})`,
      )
      .with({ _tag: 'CiWorkflowSchemaInvalid' }, (e) => `GitHub workflow: ${e.reason}`)
      .with(
        { _tag: 'CiWorkflowSchemaChanged' },
        () => 'The workflow inputs changed. Review the refreshed form before running it.',
      )
      .with(
        { _tag: 'CiRefChanged' },
        () => 'The selected GitHub ref moved. Review its new commit before running the workflow.',
      )
      .with({ _tag: 'CiDispatchCancelled' }, () => 'Workflow run cancelled before dispatch')
      .with(
        { _tag: 'CiDispatchAmbiguous' },
        () => 'GitHub may have accepted the workflow run. Check Actions before trying again.',
      )
      .with({ _tag: 'CiRateLimited' }, (e) =>
        e.resetAt
          ? `GitHub API rate limit reached until ${new Date(e.resetAt).toLocaleTimeString()}`
          : 'GitHub API rate limit reached',
      )
      .with({ _tag: 'CiApiError' }, (e) => {
        const provider = e.provider === 'github-actions' ? 'GitHub' : 'TeamCity'
        return e.status > 0
          ? `${provider} API error ${e.status}: ${e.message}`
          : `${provider}: ${e.message}`
      })
      .exhaustive()
  )
}
