export function shouldBlockAppShortcuts(dialogType: string, confirmationOpen: boolean): boolean {
  return dialogType !== 'none' || confirmationOpen
}

export function isAppShortcut(key: string, shiftKey: boolean, altKey: boolean): boolean {
  const normalized = key.toLowerCase()
  if (altKey && ['arrowleft', 'arrowright', 'arrowup', 'arrowdown'].includes(normalized)) {
    return true
  }
  if (normalized >= '1' && normalized <= '9') return true
  if (shiftKey && ['n', 'i', '[', ']'].includes(normalized)) return true
  if (normalized === 'p') return !shiftKey
  if (['b', 'l', 'o', 'w'].includes(normalized)) return !shiftKey
  return ['k', ',', 't', 'd'].includes(normalized)
}

interface AppShortcutEvent {
  key: string
  shiftKey: boolean
  altKey: boolean
  preventDefault: () => void
}

/** Returns true when MainLayout must stop routing this event to app-level actions. */
export function guardAppShortcut(
  dialogType: string,
  confirmationOpen: boolean,
  modifierPressed: boolean,
  event: AppShortcutEvent,
): boolean {
  if (!shouldBlockAppShortcuts(dialogType, confirmationOpen)) return false
  if (modifierPressed && isAppShortcut(event.key, event.shiftKey, event.altKey)) {
    event.preventDefault()
  }
  return true
}
