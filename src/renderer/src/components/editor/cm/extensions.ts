import { EditorView, keymap, lineNumbers, highlightActiveLine } from '@codemirror/view'
import { EditorState, type Extension } from '@codemirror/state'
import {
  history,
  defaultKeymap,
  historyKeymap,
  indentWithTab,
  indentMore,
  indentLess,
} from '@codemirror/commands'
import { autocompletion, completionKeymap, completeAnyWord } from '@codemirror/autocomplete'
import { search, searchKeymap } from '@codemirror/search'
import { bracketMatching, indentOnInput, foldGutter, foldKeymap } from '@codemirror/language'
import { indentationMarkers } from '@replit/codemirror-indentation-markers'

export interface BaseExtensionOpts {
  readOnly: boolean
}

export function createBaseExtensions(opts: BaseExtensionOpts): Extension[] {
  return [
    lineNumbers(),
    foldGutter(),
    highlightActiveLine(),
    bracketMatching(),
    indentOnInput(),
    indentationMarkers({
      thickness: 1,
      highlightActiveBlock: true,
      colors: {
        light: 'oklch(1 0 0 / 0.12)',
        dark: 'oklch(1 0 0 / 0.12)',
        activeLight: 'oklch(1 0 0 / 0.3)',
        activeDark: 'oklch(1 0 0 / 0.3)',
      },
    }),
    EditorView.domEventHandlers({
      keydown: (event) => {
        if (event.key === 'Tab') {
          event.stopPropagation()
        }
        return false
      },
    }),
    history(),
    autocompletion({ override: [completeAnyWord] }),
    search({ top: true }),
    EditorView.lineWrapping,
    EditorState.readOnly.of(opts.readOnly),
    keymap.of([
      indentWithTab,
      { key: 'Tab', run: indentMore, shift: indentLess },
      ...defaultKeymap,
      ...historyKeymap,
      ...searchKeymap,
      ...completionKeymap,
      ...foldKeymap,
    ]),
  ]
}
