// Cap how many untracked files `getDiffParsed` reads inline. Each read is a
// synchronous statSync+readFileSync on the main thread (see the comment on
// buildUntrackedDiffFile for why sync), and the file *count* is otherwise
// unbounded — a fresh scaffold or an `npm install` before node_modules is
// ignored can leave thousands of untracked files, and the diff refresh runs on
// every debounced files:changed event. Files past the cap are still listed in
// the changes panel, just without an inline diff body.
export const UNTRACKED_MAX_FILES = 200

/**
 * Split untracked paths into the ones whose contents get read inline and the
 * ones listed without a diff body. Every input path lands in exactly one
 * bucket, so nothing disappears from the changes list.
 */
export function splitUntrackedForDiff(files: string[]): { read: string[]; listOnly: string[] } {
  return {
    read: files.slice(0, UNTRACKED_MAX_FILES),
    listOnly: files.slice(UNTRACKED_MAX_FILES),
  }
}
