// Canopy-wide date formatting: dates are always YYYY-MM-DD (local time), timestamps add HH:mm.
// Never use toLocaleString/toLocaleDateString for dates shown in the UI.

function pad(n: number): string {
  return String(n).padStart(2, '0')
}

/** ISO-ish date → `YYYY-MM-DD` in local time. Empty string for missing/invalid input. */
export function formatDate(iso?: string | number | null): string {
  if (!iso) return ''
  const d = new Date(iso)
  if (isNaN(d.getTime())) return ''
  return `${d.getFullYear()}-${pad(d.getMonth() + 1)}-${pad(d.getDate())}`
}

/** ISO-ish date → `YYYY-MM-DD HH:mm` in local time. Empty string for missing/invalid input. */
export function formatDateTime(iso?: string | number | null): string {
  const date = formatDate(iso)
  if (!date) return ''
  const d = new Date(iso as string | number)
  return `${date} ${pad(d.getHours())}:${pad(d.getMinutes())}`
}
