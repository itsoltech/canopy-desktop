export type NetworkInterface = { name: string; address: string; virtual: boolean }
export type SelectOption = { value: string; label: string }
export type SelectGroup = { label: string; options: SelectOption[] }

export const REMOTE_LISTEN_ALL_VALUE = '__all__'

export function applyRemoteListenerPref(
  value: string,
  setPref: (key: string, value: string) => void,
): void {
  if (value === REMOTE_LISTEN_ALL_VALUE) {
    setPref('remote.listenAllInterfaces', 'true')
    return
  }
  setPref('remote.selectedInterface', value)
  setPref('remote.listenAllInterfaces', 'false')
}

export function buildRemoteInterfaceGroups(
  interfaces: NetworkInterface[],
  selectedInterface: string,
): SelectGroup[] {
  const placeholder = [{ value: '', label: 'Select adapter' }]
  const physical = interfaces
    .filter((i) => !i.virtual)
    .map((i) => ({ value: i.name, label: `${i.name} (${i.address})` }))
  const virtual = interfaces
    .filter((i) => i.virtual)
    .map((i) => ({ value: i.name, label: `${i.name} (${i.address}) virtual` }))
  const groups: SelectGroup[] = [{ label: 'Required', options: placeholder }]
  if (physical.length) groups.push({ label: 'Physical', options: physical })
  if (virtual.length) groups.push({ label: 'Virtual', options: virtual })
  if (selectedInterface && !interfaces.some((i) => i.name === selectedInterface)) {
    groups.push({
      label: 'Unavailable',
      options: [{ value: selectedInterface, label: `${selectedInterface} (not ready)` }],
    })
  }
  return groups
}

export function buildRemoteListenerGroups(
  interfaces: NetworkInterface[],
  selectedInterface: string,
): SelectGroup[] {
  const adapterGroups = buildRemoteInterfaceGroups(interfaces, selectedInterface).filter(
    (group) => group.label !== 'Required',
  )
  return [
    { label: 'Scope', options: [{ value: REMOTE_LISTEN_ALL_VALUE, label: 'All adapters' }] },
    ...adapterGroups,
  ]
}

export function formatRemoteInterfaceLabel(
  interfaces: NetworkInterface[],
  interfaceName: string,
): string {
  const found = interfaces.find((i) => i.name === interfaceName)
  return found ? `${found.name} (${found.address})` : interfaceName || 'Select adapter'
}
