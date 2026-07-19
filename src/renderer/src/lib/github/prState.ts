/** GitHub PR states → chip colors matching GitHub's own palette:
 *  open green, draft gray, merged purple, closed red. */
export function prStateChip(
  state: string | undefined,
  isDraft?: boolean,
): { label: string; cls: string } {
  if (!state) return { label: '', cls: '' }
  if (isDraft && state === 'OPEN') return { label: 'Draft', cls: 'bg-active text-text-muted' }
  if (state === 'OPEN') return { label: 'Open', cls: 'bg-success-bg text-success-text' }
  if (state === 'MERGED') return { label: 'Merged', cls: 'bg-generate-bg text-generate' }
  return { label: 'Closed', cls: 'bg-danger-bg text-danger-text' }
}
