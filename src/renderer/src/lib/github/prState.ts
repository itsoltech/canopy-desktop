import { match } from 'ts-pattern'

/** GitHub PR states → chip colors matching GitHub's own palette:
 *  open green, draft gray, merged purple, closed red. */
export function prStateChip(
  state: string | undefined,
  isDraft?: boolean,
): { label: string; cls: string } {
  if (!state) return { label: '', cls: '' }
  return match({ state, isDraft })
    .with({ state: 'OPEN', isDraft: true }, () => ({
      label: 'Draft',
      cls: 'bg-active text-text-muted',
    }))
    .with({ state: 'OPEN' }, () => ({ label: 'Open', cls: 'bg-success-bg text-success-text' }))
    .with({ state: 'MERGED' }, () => ({ label: 'Merged', cls: 'bg-generate-bg text-generate' }))
    .otherwise(() => ({ label: 'Closed', cls: 'bg-danger-bg text-danger-text' }))
}
