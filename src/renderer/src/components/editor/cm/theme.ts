import { EditorView } from '@codemirror/view'
import { HighlightStyle, syntaxHighlighting } from '@codemirror/language'
import type { Extension } from '@codemirror/state'
import { tags as t } from '@lezer/highlight'

const canopyTheme = EditorView.theme({
  '&': {
    color: 'var(--color-text)',
    backgroundColor: 'var(--color-bg)',
    height: '100%',
  },
  '.cm-content': {
    caretColor: 'var(--color-accent)',
    fontFamily:
      "var(--font-mono, ui-monospace, SFMono-Regular, 'SF Mono', Menlo, Monaco, Consolas, monospace)",
    fontSize: '13px',
  },
  '.cm-cursor, .cm-dropCursor': {
    borderLeftColor: 'var(--color-accent)',
  },
  '&.cm-focused .cm-selectionBackground, .cm-selectionBackground, ::selection': {
    backgroundColor: 'oklch(0.782 0.115 243.83 / 0.25)',
  },
  '.cm-gutters': {
    backgroundColor: 'var(--color-bg)',
    color: 'var(--color-text-muted)',
    border: 'none',
    borderRight: '1px solid var(--color-border-subtle)',
  },
  '.cm-activeLine': {
    backgroundColor: 'var(--color-hover)',
  },
  '.cm-activeLineGutter': {
    backgroundColor: 'var(--color-hover)',
    color: 'var(--color-text)',
  },
  '.cm-lineNumbers .cm-gutterElement': {
    padding: '0 8px 0 6px',
    minWidth: '2.5em',
  },
  '.cm-tooltip': {
    backgroundColor: 'var(--color-bg-elevated)',
    color: 'var(--color-text)',
    border: '1px solid var(--color-border-subtle)',
  },
  '.cm-tooltip-autocomplete > ul > li[aria-selected]': {
    backgroundColor: 'var(--color-hover)',
    color: 'var(--color-text)',
  },
  '.cm-matchingBracket, .cm-nonmatchingBracket': {
    backgroundColor: 'var(--color-hover)',
    outline: '1px solid var(--color-border-subtle)',
  },
  '.cm-panels': {
    backgroundColor: 'var(--color-bg-elevated)',
    color: 'var(--color-text)',
    fontFamily: 'var(--font-sans)',
  },
  '.cm-panels.cm-panels-top': {
    borderBottom: '1px solid var(--color-border-subtle)',
  },
  '.cm-panel.cm-search': {
    padding: '6px 32px 6px 10px',
    fontSize: '12px',
    lineHeight: '1.6',
  },
  '.cm-panel.cm-search input, .cm-panel.cm-search input.cm-textfield': {
    background: 'var(--color-bg-input)',
    color: 'var(--color-text)',
    border: '1px solid var(--color-border)',
    borderRadius: '4px',
    padding: '3px 8px',
    fontFamily: 'var(--font-mono)',
    fontSize: '12px',
    outline: 'none',
    width: '220px',
    verticalAlign: 'middle',
  },
  '.cm-panel.cm-search input:focus, .cm-panel.cm-search input.cm-textfield:focus': {
    borderColor: 'var(--color-accent)',
    boxShadow: '0 0 0 2px var(--color-focus-ring)',
  },
  '.cm-panel.cm-search input::placeholder': {
    color: 'var(--color-text-muted)',
  },
  '.cm-panel.cm-search input[type="checkbox"]': {
    accentColor: 'var(--color-accent)',
    cursor: 'pointer',
    width: '13px',
    height: '13px',
    margin: '0 4px 0 0',
    verticalAlign: '-2px',
  },
  '.cm-panel.cm-search label': {
    color: 'var(--color-text-secondary)',
    fontSize: '12px',
    cursor: 'pointer',
    userSelect: 'none',
    marginRight: '8px',
    whiteSpace: 'nowrap',
  },
  '.cm-panel.cm-search br': {
    lineHeight: '0',
  },
  '.cm-panel.cm-search button, .cm-panel.cm-search button.cm-button': {
    background: 'var(--color-bg-input)',
    color: 'var(--color-text)',
    border: '1px solid var(--color-border-subtle)',
    borderRadius: '4px',
    padding: '3px 10px',
    fontSize: '12px',
    fontFamily: 'inherit',
    cursor: 'pointer',
    backgroundImage: 'none',
    textTransform: 'none',
    margin: '0 4px 0 0',
    verticalAlign: 'middle',
  },
  '.cm-panel.cm-search button:hover, .cm-panel.cm-search button.cm-button:hover': {
    background: 'var(--color-hover-strong)',
    borderColor: 'var(--color-border)',
  },
  '.cm-panel.cm-search button:active, .cm-panel.cm-search button.cm-button:active': {
    background: 'var(--color-active)',
  },
  '.cm-panel.cm-search button:focus-visible': {
    outline: 'none',
    boxShadow: '0 0 0 2px var(--color-focus-ring)',
  },
  '.cm-panel.cm-search button[name="close"]': {
    position: 'absolute',
    top: '4px',
    right: '4px',
    background: 'transparent',
    border: 'none',
    color: 'var(--color-text-muted)',
    fontSize: '16px',
    lineHeight: '1',
    padding: '2px 6px',
    minWidth: 'unset',
  },
  '.cm-panel.cm-search button[name="close"]:hover': {
    background: 'var(--color-hover)',
    color: 'var(--color-text)',
    borderColor: 'transparent',
  },
  '.cm-searchMatch': {
    backgroundColor: 'color-mix(in srgb, var(--color-warning) 22%, transparent)',
    borderRadius: '2px',
  },
  '.cm-searchMatch.cm-searchMatch-selected': {
    backgroundColor: 'color-mix(in srgb, var(--color-warning) 45%, transparent)',
    outline: '1px solid color-mix(in srgb, var(--color-warning) 70%, transparent)',
  },
})

const highlightStyle = HighlightStyle.define([
  {
    tag: t.keyword,
    color: 'var(--color-syntax-keyword)',
  },
  {
    tag: [t.string, t.special(t.string)],
    color: 'var(--color-syntax-string)',
  },
  {
    tag: t.comment,
    color: 'var(--color-syntax-comment)',
    fontStyle: 'italic',
  },
  {
    tag: [t.number, t.bool, t.null],
    color: 'var(--color-syntax-number)',
  },
  {
    tag: t.operator,
    color: 'var(--color-syntax-operator)',
  },
  {
    tag: [t.function(t.variableName), t.function(t.propertyName)],
    color: 'var(--color-syntax-function)',
  },
  {
    tag: t.variableName,
    color: 'var(--color-syntax-variable)',
  },
  {
    tag: [t.propertyName, t.attributeName],
    color: 'var(--color-syntax-property)',
  },
  {
    tag: [t.typeName, t.className],
    color: 'var(--color-syntax-type)',
  },
  {
    tag: [t.tagName, t.angleBracket],
    color: 'var(--color-syntax-tag)',
  },
  {
    tag: t.heading,
    color: 'var(--color-syntax-heading)',
    fontWeight: 'bold',
  },
  {
    tag: [t.link, t.url],
    color: 'var(--color-accent)',
    textDecoration: 'underline',
  },
  {
    tag: t.invalid,
    color: 'var(--color-syntax-error)',
  },
])

export function createCanopyTheme(): Extension {
  return [canopyTheme, syntaxHighlighting(highlightStyle)]
}
