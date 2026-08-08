// Pure state helpers for the "Run build" parameter dialog. Form state is a flat
// name → string-value map — exactly what the TeamCity property list wants — and
// these helpers translate between that and the richer widgets (checkbox with custom
// checked/unchecked values, multi-select joined by the spec's separator).

export function initialFormValues(params: CiParameter[]): Record<string, string> {
  return Object.fromEntries(params.map((p) => [p.name, initialValue(p)]))
}

function initialValue(p: CiParameter): string {
  if (p.kind !== 'checkbox') return p.defaultValue
  // Mirror TeamCity's own dialog: a checkbox always submits checkedValue or
  // uncheckedValue, never the raw stored value. Configs may carry a whole CLI
  // fragment in uncheckedValue (e.g. gakko's Affected → '-Site %Sites%'), so an
  // untouched unchecked checkbox submitting '' would silently drop that fragment.
  return isCheckboxChecked(p, p.defaultValue)
    ? (p.checkedValue ?? 'true')
    : (p.uncheckedValue ?? '')
}

export function isCheckboxChecked(param: CiParameter, value: string): boolean {
  return value === (param.checkedValue ?? 'true')
}

export function toggleCheckbox(param: CiParameter, value: string): string {
  return isCheckboxChecked(param, value)
    ? (param.uncheckedValue ?? '')
    : (param.checkedValue ?? 'true')
}

export function multiValues(param: CiParameter, value: string): string[] {
  return value.split(param.valueSeparator).filter((v) => v !== '')
}

export function toggleMultiValue(param: CiParameter, value: string, option: string): string {
  const current = multiValues(param, value)
  const next = current.includes(option) ? current.filter((v) => v !== option) : [...current, option]
  return next.join(param.valueSeparator)
}

export function missingRequired(params: CiParameter[], values: Record<string, string>): string[] {
  return params
    .filter((p) => p.required && !p.hasDefault && (values[p.name] ?? '').trim() === '')
    .map((p) => p.name)
}

export function toProperties(
  params: CiParameter[],
  values: Record<string, string>,
): Array<{ name: string; value: string }> {
  return (
    params
      // An untouched password prompt is OMITTED, not sent empty: `parsePromptParameters`
      // deliberately blanks its default, and a property present in the trigger payload
      // overrides the configuration's stored value — sending '' would deploy with an
      // empty secret. Leaving it out is what TeamCity's own dialog does.
      .filter((p) => !(p.kind === 'password' && (values[p.name] ?? '') === ''))
      .map((p) => ({ name: p.name, value: values[p.name] ?? '' }))
  )
}

export function toInputs(
  params: CiParameter[],
  values: Record<string, string>,
): Record<string, string | boolean> {
  const entries: Array<[string, string | boolean]> = []
  for (const param of params) {
    const value = values[param.name] ?? ''
    if (param.valueType === 'boolean') {
      if (value !== 'true' && value !== 'false') {
        throw new Error(`Invalid boolean value for ${param.name}`)
      }
      entries.push([param.name, value === 'true'])
    } else if (value !== '' || (param.required && !param.hasDefault)) {
      entries.push([param.name, value])
    }
  }
  return Object.fromEntries(entries)
}

/**
 * What the user actually changed, for the confirmation step. `toProperties` submits EVERY
 * prompt parameter, so a configuration with dozens of them would bury the one that matters.
 *
 * Deliberately carries no interpretation of what any parameter MEANS — names belong to each
 * server's own build configurations, so anything keyed off a specific one (a `Deploy` flag,
 * an `Environment` value) would read correctly for one installation and mislead for the next.
 */
export function changedProperties(
  params: CiParameter[],
  properties: Array<{ name: string; value: string }>,
): Array<{ name: string; value: string }> {
  const byName = new Map(params.map((p) => [p.name, p]))
  return properties
    .filter((property) => {
      const param = byName.get(property.name)
      // Nothing to compare against counts as changed: a confirmation must fail towards
      // showing more, never towards hiding.
      return param === undefined || param.defaultValue !== property.value
    })
    .map((property) => ({
      name: property.name,
      // Never echo a secret back, even one just typed.
      value: byName.get(property.name)?.kind === 'password' ? '********' : property.value,
    }))
}
