// Renderer-side mirror of the preload `CiParameter` shape — ambient declarations
// from index.d.ts are not visible to ESLint inside .svelte scripts, so components
// import this structurally-identical type instead.
export interface CiParameter {
  name: string
  kind: 'text' | 'checkbox' | 'select'
  label: string
  description: string | undefined
  required: boolean
  defaultValue: string
  options: string[] | undefined
  multiple: boolean
  valueSeparator: string
  checkedValue: string | undefined
  uncheckedValue: string | undefined
}
