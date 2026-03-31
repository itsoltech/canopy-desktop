export const prefs: Record<string, string> = $state({})
export let prefsReady: boolean = $state(false)

export async function loadPrefs(): Promise<void> {
  const all = await window.api.getAllPrefs()
  Object.assign(prefs, all)
  prefsReady = true
}

export async function setPref(key: string, value: string): Promise<void> {
  prefs[key] = value
  await window.api.setPref(key, value)
}

export function getPref(key: string, defaultValue: string = ''): string {
  return prefs[key] ?? defaultValue
}
