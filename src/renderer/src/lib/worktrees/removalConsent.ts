import { confirm } from '../stores/dialogs.svelte'

export { removalNeedsForceConsent } from './removalGuard'

export interface WorktreeRemovalPreflight {
  hasUncommittedChanges: boolean
  unmergedCommitCount: number
  branchMerged: boolean
  forceRequired: boolean
  canDeleteBranch: boolean
  warnings: string[]
}

export interface WorktreeRemovalConsent {
  ok: boolean
  /** Destructive consent collected — authorizes --force for dirty/force-required refusals. */
  force: boolean
  preflight: WorktreeRemovalPreflight | null
}

/**
 * The one preflight/consent gate used by every local removal entry point (sidebar,
 * project tree, command palette). Runs `worktree:prepareRemove` FIRST and puts its
 * warnings in the confirmation, so destructive consent is informed and collected
 * before any teardown. When the preflight itself fails — e.g. a ghost worktree
 * whose broken `.git` link makes `git status` impossible — it falls back to a
 * generic destructive confirmation with force consent, which is exactly what the
 * broken-link removal path requires.
 */
export async function confirmWorktreeRemoval(args: {
  repoRoot: string
  worktreePath: string
  branch: string
  /** Extra sentence appended to the confirmation (e.g. branch deletion notice). */
  detailSuffix?: string
}): Promise<WorktreeRemovalConsent> {
  let preflight: WorktreeRemovalPreflight | null = null
  try {
    preflight = await window.api.worktreePrepareRemove({
      repoRoot: args.repoRoot,
      worktreePath: args.worktreePath,
      branch: args.branch,
    })
  } catch {
    // Ghost/broken worktrees cannot be preflighted — git cannot verify their
    // state, so the confirmation below is explicitly destructive.
  }

  const warnings = preflight?.warnings ?? [
    'The worktree state could not be verified (broken or corrupted checkout) — files inside may include unsaved work.',
  ]
  const forceRequired = preflight?.forceRequired ?? true

  const lines = forceRequired ? [...warnings, ''] : []
  lines.push(`Remove worktree "${args.branch}"?`)
  if (args.detailSuffix) lines.push(args.detailSuffix)

  const ok = await confirm({
    title: 'Remove Worktree',
    message: lines.join('\n'),
    details: args.worktreePath,
    confirmLabel: 'Remove',
    destructive: forceRequired,
  })

  return { ok, force: forceRequired, preflight }
}
