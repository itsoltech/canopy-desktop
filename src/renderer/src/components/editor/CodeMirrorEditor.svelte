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
    initialIndentUnit = '    ',
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

  // Per-file EditorStates so each sub-tab has its own undo history + cursor.
  // Not reactive — only accessed imperatively from exported methods.
  // eslint-disable-next-line svelte/prefer-svelte-reactivity
  const buffers = new Map<string, EditorState>()
  let activeBufferKey: string | null = null
  let extensionsRef: Extension[] | null = null

  function makeFreshState(doc: string): EditorState {
    if (!extensionsRef) {
      throw new Error('CodeMirrorEditor not mounted')
    }
    return EditorState.create({ doc, extensions: extensionsRef })
  }

  onMount(() => {
    if (!container) return
    let cancelled = false

    extensionsRef = [
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

    const initialState = makeFreshState(mountValue)
    buffers.set(mountFilePath, initialState)
    activeBufferKey = mountFilePath

    view = new EditorView({ parent: container, state: initialState })

    void (async () => {
      const lang = await detectLanguage(mountFilePath)
      if (cancelled || !view || !lang) return
      view.dispatch({ effects: languageCompartment.reconfigure(lang) })
    })()

    return () => {
      cancelled = true
      view?.destroy()
      view = null
      buffers.clear()
      extensionsRef = null
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

  function saveActiveBuffer(): void {
    if (!view || !activeBufferKey) return
    buffers.set(activeBufferKey, view.state)
  }

  export function focus(): void {
    view?.focus()
  }

  /** Switch (or create) a buffer for filePath. If a buffer exists, restores
   * its full state (doc + history). Otherwise creates a fresh one with `doc`. */
  export function openBuffer(newFilePath: string, doc: string, unit?: string): void {
    if (!view || !extensionsRef) return
    saveActiveBuffer()
    activeBufferKey = newFilePath
    const existing = buffers.get(newFilePath)
    if (existing) {
      view.setState(existing)
    } else {
      const fresh = makeFreshState(doc)
      buffers.set(newFilePath, fresh)
      view.setState(fresh)
    }
    if (unit !== undefined) setIndentUnit(unit)
    void setLanguage(newFilePath)
  }

  /** Replace the active buffer with a freshly-seeded state (clears history).
   * Used for explicit reloads (refresh, conflict resolution, initial disk load). */
  export function reloadBuffer(doc: string, unit?: string): void {
    if (!view || !extensionsRef || !activeBufferKey) return
    const fresh = makeFreshState(doc)
    buffers.set(activeBufferKey, fresh)
    view.setState(fresh)
    if (unit !== undefined) setIndentUnit(unit)
    void setLanguage(activeBufferKey)
  }

  /** Drop a buffer entirely; call when a sub-tab closes. */
  export function closeBuffer(key: string): void {
    buffers.delete(key)
    if (activeBufferKey === key) activeBufferKey = null
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

  let lastLanguagePath: string | null = null
  export async function setLanguage(newFilePath: string): Promise<void> {
    if (!view) return
    if (newFilePath === lastLanguagePath) return
    lastLanguagePath = newFilePath
    const lang = await detectLanguage(newFilePath)
    if (!view) return
    if (newFilePath !== lastLanguagePath) return
    view.dispatch({
      effects: languageCompartment.reconfigure(lang ?? []),
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

<div bind:this={container} class="cm-root w-full h-full overflow-hidden"></div>

<!-- :global() rules required to style CodeMirror's internal classes (.cm-editor, .cm-scroller).
     These selectors don't exist in our markup so cannot be expressed as Tailwind utilities. -->