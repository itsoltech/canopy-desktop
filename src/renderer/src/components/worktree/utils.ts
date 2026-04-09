/**
 * Returns true when `branch` is a remote-tracking ref (e.g. "origin/feature-x")
 * and no local branch with the same short name exists. Used to surface
 * "remote only" branches in the picker and to decide whether
 * `git worktree add` needs `-b <localName>` to create a tracking branch.
 */
export function isRemoteOnly(
  branch: string,
  branches: { local: string[]; remote: string[] },
): boolean {
  if (!branches.remote.includes(branch)) return false
  const localName = branch.slice(branch.indexOf('/') + 1)
  return !branches.local.includes(localName)
}
