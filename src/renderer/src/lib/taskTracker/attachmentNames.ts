/**
 * Returns `name`, or `name` numbered before its extension (`image-2.png`) when it is already
 * taken. Description markers reference attachments by file name, so two pastes both called
 * `image.png` would otherwise point at the same upload.
 */
export function uniqueAttachmentName(name: string, taken: readonly string[]): string {
  if (!taken.includes(name)) return name
  const dot = name.lastIndexOf('.')
  const hasExtension = dot > 0
  const stem = hasExtension ? name.slice(0, dot) : name
  const extension = hasExtension ? name.slice(dot) : ''
  for (let n = 2; ; n++) {
    const candidate = `${stem}-${n}${extension}`
    if (!taken.includes(candidate)) return candidate
  }
}
