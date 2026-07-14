import { match } from 'ts-pattern'

/**
 * Chip colors follow the tracker's status category (Jira: To Do / In Progress / Done; other
 * providers approximate). Unknown/missing categories get the neutral chip.
 */
export function statusChipClass(category?: string): string {
  return match(category)
    .with('done', () => 'bg-success-bg text-success-text')
    .with('in-progress', () => 'bg-accent-bg text-accent-text')
    .otherwise(() => 'bg-active text-text-muted')
}
