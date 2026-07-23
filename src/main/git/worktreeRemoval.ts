/**
 * Failure taxonomy for `git worktree remove` on real filesystems (esp. Windows).
 *
 * A removal that races dying PTY shells, editors, or AV scanners fails in ways that
 * need OPPOSITE reactions: a lock is transient (retry with backoff), while
 * "is not a working tree" means a previous attempt already unregistered the
 * worktree — retrying git is pointless and the remaining work is filesystem
 * cleanup. Blindly force-retrying (the old behavior) turned partial successes into
 * hard failures and left stray branches and ghost folders behind.
 */
export type WorktreeRemoveErrorKind = 'already-removed' | 'locked' | 'dirty' | 'other'

export function classifyWorktreeRemoveError(message: string): WorktreeRemoveErrorKind {
  if (/is not a working tree/i.test(message)) return 'already-removed'
  if (/contains modified or untracked files/i.test(message)) return 'dirty'
  if (
    /unable to unlink|failed to delete|Permission denied|Access is denied|Directory not empty|Device or resource busy|EBUSY|EPERM|EACCES|Invalid argument/i.test(
      message,
    )
  ) {
    return 'locked'
  }
  return 'other'
}

/** Backoff schedule for lock-classified retries: dying process trees on Windows
 *  release their cwd/file handles within a couple of seconds. */
export const REMOVE_RETRY_DELAYS_MS = [500, 1000, 1500, 2000]
