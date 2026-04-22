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
    backgroundColor: 'var(--c-surface, #1e1e1e)',
    color: 'var(--c-text)',
  },
  '.cm-panels.cm-panels-top': {
    borderBottom: '1px solid var(--c-border-subtle, rgba(255, 255, 255, 0.08))',
  },
  '.cm-searchMatch': {
    backgroundColor: 'var(--c-warning-bg, rgba(255, 200, 0, 0.2))',
  },
  '.cm-searchMatch.cm-searchMatch-selected': {
    backgroundColor: 'var(--c-warning-bg, rgba(255, 200, 0, 0.4))',
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
