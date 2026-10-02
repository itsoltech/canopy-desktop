function escapeHtml(str: string): string {
  return str
    .replace(/&/g, '&amp;')
    .replace(/</g, '&lt;')
    .replace(/>/g, '&gt;')
    .replace(/"/g, '&quot;')
}

function escapeRegex(str: string): string {
  return str.replace(/[.*+?^${}()|[\]\\]/g, '\\$&')
}

/** HTML for a diff line with every case-insensitive occurrence of `query` wrapped in <mark>. */
export function highlightMatch(text: string, query: string): string {
  if (!query) return escapeHtml(text)
  // Match on the raw text and escape each piece; matching the escaped text let a query such
  // as "lt" land inside "&lt;" and show the entity literally.
  const regex = new RegExp(`(${escapeRegex(query)})`, 'gi')
  return text
    .split(regex)
    .map((part, i) =>
      i % 2 === 1 ? `<mark class="search-highlight">${escapeHtml(part)}</mark>` : escapeHtml(part),
    )
    .join('')
}
