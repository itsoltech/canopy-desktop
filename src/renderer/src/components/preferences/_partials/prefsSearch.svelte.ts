export const prefsSearch: { query: string } = $state({ query: '' })

export function setQuery(q: string): void {
  prefsSearch.query = q
}

export function clearQuery(): void {
  prefsSearch.query = ''
}

export function matches(text: string | undefined): boolean {
  const q = prefsSearch.query.trim().toLowerCase()
  if (!q) return true
  if (!text) return false
  return text.toLowerCase().includes(q)
}
