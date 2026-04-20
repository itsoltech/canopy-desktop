export function safeDirName(name: string): string {
  return (
    name
      .replace(/\//g, '-')
      .replace(/[#~^:?*[\]\\@{}<>|"'!$&()`;,\s]/g, '-')
      .replace(/-+/g, '-')
      .replace(/^-|-$/g, '') || 'unnamed'
  )
}
