export type NetworkInterface = { name: string; address: string; virtual: boolean }
export type SelectOption = { value: string; label: string }
export type SelectGroup = { label: string; options: SelectOption[] }

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
