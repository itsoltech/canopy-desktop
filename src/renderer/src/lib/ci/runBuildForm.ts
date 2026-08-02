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
  return params.filter((p) => p.required && (values[p.name] ?? '').trim() === '').map((p) => p.name)
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
