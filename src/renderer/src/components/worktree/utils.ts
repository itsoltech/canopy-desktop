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

/**
 * Whether the combobox renders expanded on its FIRST frame. `startCollapsed` beats every
 * other condition, including "nothing selected yet" — which is otherwise the strongest
 * reason to open, and would leave a trigger dialog covered by a full-height branch list
 * before the user has asked to see one.
 */
export function initialBranchListOpen(
  startCollapsed: boolean,
  collapseConfirmedSelection: boolean,
  selectedBranch: string,
  query: string,
): boolean {
  if (startCollapsed) return false
  return !collapseConfirmedSelection || !selectedBranch || query !== selectedBranch
}

/** A confirmed combobox selection only reopens when its bound value is reset or edited. */
export function shouldReopenBranchList(
  collapseConfirmedSelection: boolean,
  selectedBranch: string,
  query: string,
): boolean {
  return collapseConfirmedSelection && (!selectedBranch || query !== selectedBranch)
}

/** A bounded ref list must not hide the explicit lookup for an exact typed name. */
export function shouldOfferExactRef(query: string, loadedRefs: string[]): boolean {
  const exactName = query.trim()
  return exactName.length > 0 && !loadedRefs.includes(exactName)
}

/** Enter preserves listbox navigation; a literal loaded ref wins over the active fuzzy row. */
export function branchPickerEnterTarget(
  query: string,
  loadedRefs: string[],
  filteredRefs: string[],
  selectedIndex: number,
): string | null {
  return loadedRefs.find((ref) => ref === query.trim()) ?? filteredRefs[selectedIndex] ?? null
}
