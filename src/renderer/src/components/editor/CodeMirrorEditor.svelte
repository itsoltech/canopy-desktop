<script lang="ts">
  import { onMount } from 'svelte'
  import { EditorView, keymap } from '@codemirror/view'
  import { EditorState, Compartment, type Extension } from '@codemirror/state'
  import { indentUnit } from '@codemirror/language'
  import { createBaseExtensions } from './cm/extensions'
  import { detectLanguage } from './cm/language'
  import { createCanopyTheme } from './cm/theme'

  interface Props {
    initialValue: string
    initialIndentUnit?: string
    filePath: string
    readOnly?: boolean
    onSave?: () => void
    onChange?: (value: string) => void
  }

  let {
    initialValue,
    initialIndentUnit = '  ',
    filePath,
    readOnly = false,
    onSave,
    onChange,
  }: Props = $props()

  let container: HTMLDivElement | undefined = $state()
  let view: EditorView | null = null
  let isApplyingExternal = false

  const languageCompartment = new Compartment()
  const readOnlyCompartment = new Compartment()
  const indentCompartment = new Compartment()

  const mountValue = initialValue
  const mountReadOnly = readOnly
  const mountFilePath = filePath
  const mountIndentUnit = initialIndentUnit

  onMount(() => {
    if (!container) return
    let cancelled = false

    const baseExtensions: Extension[] = [
      ...createBaseExtensions({ readOnly: mountReadOnly }),
      createCanopyTheme(),
      languageCompartment.of([]),
      readOnlyCompartment.of(EditorState.readOnly.of(mountReadOnly)),
      indentCompartment.of(indentUnit.of(mountIndentUnit)),
      EditorView.updateListener.of((update) => {
        if (isApplyingExternal || !update.docChanged) return
        onChange?.(update.state.doc.toString())
      }),
      keymap.of([
        {
          key: 'Mod-s',
          preventDefault: true,
          run: () => {
            onSave?.()
            return true
          },
        },
      ]),
    ]

    view = new EditorView({
      parent: container,
      state: EditorState.create({
        doc: mountValue,
        extensions: baseExtensions,
      }),
    })

    void (async () => {
      const lang = await detectLanguage(mountFilePath)
      if (cancelled || !view || !lang) return
      view.dispatch({
        effects: languageCompartment.reconfigure(lang),
      })
    })()

    return () => {
      cancelled = true
      view?.destroy()
      view = null
    }
  })

  let lastReadOnly = mountReadOnly
  $effect(() => {
    if (!view) return
    if (readOnly === lastReadOnly) return
    lastReadOnly = readOnly
    view.dispatch({
      effects: readOnlyCompartment.reconfigure(EditorState.readOnly.of(readOnly)),
    })
  })

  export function focus(): void {
    view?.focus()
  }

  export function setContent(newValue: string): void {
    if (!view) return
    if (newValue === view.state.doc.toString()) return
    isApplyingExternal = true
    try {
      view.dispatch({
        changes: { from: 0, to: view.state.doc.length, insert: newValue },
      })
    } finally {
      isApplyingExternal = false
    }
  }

  export function setIndentUnit(unit: string): void {
    if (!view) return
    view.dispatch({
      effects: indentCompartment.reconfigure(indentUnit.of(unit)),
    })
  }

  export function goToLine(lineNumber: number): void {
    if (!view) return
    const doc = view.state.doc
    const clampedLine = Math.max(1, Math.min(lineNumber, doc.lines))
    const line = doc.line(clampedLine)
    view.dispatch({
      selection: { anchor: line.from },
      scrollIntoView: true,
    })
    view.focus()
  }
</script>

<div bind:this={container} class="cm-root"></div>

<style>
  .cm-root {
    width: 100%;
    height: 100%;
    overflow: hidden;
  }
  .cm-root :global(.cm-editor) {
    height: 100%;
  }
  .cm-root :global(.cm-editor.cm-focused) {
    outline: none;
  }
  .cm-root :global(.cm-scroller) {
    overflow: auto;
  }
</style>
