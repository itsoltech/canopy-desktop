/** Strip chars unsafe for directory names; collapse repeated dashes */
export function safeDirName(name: string): string {
  return name
    .replace(/\//g, '-')
    .replace(/[#~^:?*[\]\\@{}<>|"'!$&()`;,\s]/g, '-')
    .replace(/-+/g, '-')
    .replace(/^-|-$/g, '')
}
