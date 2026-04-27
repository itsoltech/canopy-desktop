import { EditorView } from '@codemirror/view'
import { HighlightStyle, syntaxHighlighting } from '@codemirror/language'
import type { Extension } from '@codemirror/state'
import { tags as t } from '@lezer/highlight'

const canopyTheme = EditorView.theme({
  '&': {
    color: 'var(--c-text)',
    backgroundColor: 'var(--c-bg)',
    height: '100%',
  },
  '.cm-content': {
    caretColor: 'var(--c-accent, #4a9eff)',
    fontFamily:
      "var(--font-mono, ui-monospace, SFMono-Regular, 'SF Mono', Menlo, Monaco, Consolas, monospace)",
    fontSize: '13px',
  },
  '.cm-cursor, .cm-dropCursor': {
    borderLeftColor: 'var(--c-accent, #4a9eff)',
  },
  '&.cm-focused .cm-selectionBackground, .cm-selectionBackground, ::selection': {
    backgroundColor: 'var(--c-selection, rgba(74, 158, 255, 0.25))',
  },
  '.cm-gutters': {
    backgroundColor: 'var(--c-bg)',
    color: 'var(--c-text-muted, rgba(200, 200, 200, 0.5))',
    border: 'none',
    borderRight: '1px solid var(--c-border-subtle, rgba(255, 255, 255, 0.08))',
  },
  '.cm-activeLine': {
    backgroundColor: 'var(--c-hover, rgba(255, 255, 255, 0.04))',
  },
  '.cm-activeLineGutter': {
    backgroundColor: 'var(--c-hover, rgba(255, 255, 255, 0.04))',
    color: 'var(--c-text)',
  },
  '.cm-lineNumbers .cm-gutterElement': {
    padding: '0 8px 0 6px',
    minWidth: '2.5em',
  },
  '.cm-tooltip': {
    backgroundColor: 'var(--c-surface, #1e1e1e)',
    color: 'var(--c-text)',
    border: '1px solid var(--c-border-subtle, rgba(255, 255, 255, 0.08))',
  },
  '.cm-tooltip-autocomplete > ul > li[aria-selected]': {
    backgroundColor: 'var(--c-hover, rgba(255, 255, 255, 0.08))',
    color: 'var(--c-text)',
  },
  '.cm-matchingBracket, .cm-nonmatchingBracket': {
    backgroundColor: 'var(--c-hover, rgba(255, 255, 255, 0.08))',
    outline: '1px solid var(--c-border-subtle, rgba(255, 255, 255, 0.2))',
  },
  '.cm-panels': {
    backgroundColor: 'var(--c-bg-elevated)',
    color: 'var(--c-text)',
    fontFamily: 'var(--font-sans)',
  },
  '.cm-panels.cm-panels-top': {
    borderBottom: '1px solid var(--c-border-subtle)',
  },
  '.cm-panel.cm-search': {
    padding: '6px 32px 6px 10px',
    fontSize: '12px',
    lineHeight: '1.6',
  },
  '.cm-panel.cm-search input, .cm-panel.cm-search input.cm-textfield': {
    background: 'var(--c-bg-input)',
    color: 'var(--c-text)',
    border: '1px solid var(--c-border)',
    borderRadius: '4px',
    padding: '3px 8px',
    fontFamily: 'var(--font-mono)',
    fontSize: '12px',
    outline: 'none',
    width: '220px',
    verticalAlign: 'middle',
  },
  '.cm-panel.cm-search input:focus, .cm-panel.cm-search input.cm-textfield:focus': {
    borderColor: 'var(--c-accent)',
    boxShadow: '0 0 0 2px var(--c-focus-ring)',
  },
  '.cm-panel.cm-search input::placeholder': {
    color: 'var(--c-text-muted)',
  },
  '.cm-panel.cm-search input[type="checkbox"]': {
    accentColor: 'var(--c-accent)',
    cursor: 'pointer',
    width: '13px',
    height: '13px',
    margin: '0 4px 0 0',
    verticalAlign: '-2px',
  },
  '.cm-panel.cm-search label': {
    color: 'var(--c-text-secondary)',
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
    background: 'var(--c-bg-input)',
    color: 'var(--c-text)',
    border: '1px solid var(--c-border-subtle)',
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
    background: 'var(--c-hover-strong)',
    borderColor: 'var(--c-border)',
  },
  '.cm-panel.cm-search button:active, .cm-panel.cm-search button.cm-button:active': {
    background: 'var(--c-active)',
  },
  '.cm-panel.cm-search button:focus-visible': {
    outline: 'none',
    boxShadow: '0 0 0 2px var(--c-focus-ring)',
  },
  '.cm-panel.cm-search button[name="close"]': {
    position: 'absolute',
    top: '4px',
    right: '4px',
    background: 'transparent',
    border: 'none',
    color: 'var(--c-text-muted)',
    fontSize: '16px',
    lineHeight: '1',
    padding: '2px 6px',
    minWidth: 'unset',
  },
  '.cm-panel.cm-search button[name="close"]:hover': {
    background: 'var(--c-hover)',
    color: 'var(--c-text)',
    borderColor: 'transparent',
  },
  '.cm-searchMatch': {
    backgroundColor: 'color-mix(in srgb, var(--c-warning) 22%, transparent)',
    borderRadius: '2px',
  },
  '.cm-searchMatch.cm-searchMatch-selected': {
    backgroundColor: 'color-mix(in srgb, var(--c-warning) 45%, transparent)',
    outline: '1px solid color-mix(in srgb, var(--c-warning) 70%, transparent)',
  },
})

const highlightStyle = HighlightStyle.define([
  {
    tag: t.keyword,
    color: 'var(--c-syntax-keyword, #c586c0)',
  },
  {
    tag: [t.string, t.special(t.string)],
    color: 'var(--c-syntax-string, #ce9178)',
  },
  {
    tag: t.comment,
    color: 'var(--c-syntax-comment, #6a9955)',
    fontStyle: 'italic',
  },
  {
    tag: [t.number, t.bool, t.null],
    color: 'var(--c-syntax-number, #b5cea8)',
  },
  {
    tag: t.operator,
    color: 'var(--c-syntax-operator, #d4d4d4)',
  },
  {
    tag: [t.function(t.variableName), t.function(t.propertyName)],
    color: 'var(--c-syntax-function, #dcdcaa)',
  },
  {
    tag: t.variableName,
    color: 'var(--c-syntax-variable, #9cdcfe)',
  },
  {
    tag: [t.propertyName, t.attributeName],
    color: 'var(--c-syntax-property, #9cdcfe)',
  },
  {
    tag: [t.typeName, t.className],
    color: 'var(--c-syntax-type, #4ec9b0)',
  },
  {
    tag: [t.tagName, t.angleBracket],
    color: 'var(--c-syntax-tag, #569cd6)',
  },
  {
    tag: t.heading,
    color: 'var(--c-syntax-heading, #569cd6)',
    fontWeight: 'bold',
  },
  {
    tag: [t.link, t.url],
    color: 'var(--c-accent, #4a9eff)',
    textDecoration: 'underline',
  },
  {
    tag: t.invalid,
    color: 'var(--c-error-text, #f48771)',
  },
])

export function createCanopyTheme(): Extension {
  return [canopyTheme, syntaxHighlighting(highlightStyle)]
}
