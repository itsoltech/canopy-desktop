/**
 * Why a profile's "Settings JSON override" cannot be saved, or null when it is empty or a JSON
 * object. Main ignores an unparseable override at launch without telling anyone, so it has to
 * be caught while the user is still in the form.
 */
export function settingsJsonError(raw: string | undefined): string | null {
  if (!raw?.trim()) return null
  let parsed: unknown
  try {
    parsed = JSON.parse(raw)
  } catch (e) {
    return `Settings JSON override is not valid JSON: ${e instanceof Error ? e.message : String(e)}`
  }
  if (typeof parsed !== 'object' || parsed === null || Array.isArray(parsed)) {
    return 'Settings JSON override must be a JSON object'
  }
  return null
}
