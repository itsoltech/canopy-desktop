export const prefs: Record<string, string> = $state({})

const _ready = $state({ value: false })
export function isPrefsReady(): boolean {
  return _ready.value
}

export async function loadPrefs(): Promise<void> {
  const all = await window.api.getAllPrefs()
  Object.assign(prefs, all)
  _ready.value = true
}

export async function setPref(key: string, value: string): Promise<void> {
  prefs[key] = value
  await window.api.setPref(key, value)
}

export function getPref(key: string, defaultValue: string = ''): string {
  return prefs[key] ?? defaultValue
}
