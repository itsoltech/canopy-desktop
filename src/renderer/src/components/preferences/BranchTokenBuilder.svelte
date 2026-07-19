<script lang="ts">
  // Hybrid template editor: {field} placeholders render as draggable chips, and everything BETWEEN
  // them is plain text edited in place — so any separator works (/ - _ . space [ ] # …), matching
  // what the renderer actually supports. No separate visual/manual modes.
  //
  // Model: fields[] (placeholder keys) interleaved with texts[] (literals), where
  // texts.length === fields.length + 1. The template string is the single source of truth; both
  // arrays are re-derived from it after every change.
  let {
    templateInput = $bindable(''),
    placeholders,
    onSave,
    label = 'Template',
    autoSeparators = true,
  }: {
    templateInput: string
    placeholders: Array<{ key: string; description: string; example: string }>
    onSave: () => void
    label?: string
    autoSeparators?: boolean
  } = $props()

  import { tick } from 'svelte'

  // Default separator inserted when a field is appended right after another one.
  let defaultSep = $derived(autoSeparators ? '/' : ' ')

  let trackEl: HTMLElement | undefined = $state()

  // Typing in a text segment updates the template on every keystroke; persisting each keystroke
  // would hammer the config file, so those saves are debounced. Discrete actions (add/remove/move
  // a chip) save immediately and flush anything pending.
  let saveTimer: ReturnType<typeof setTimeout> | null = null
  let savePending = false

  function flushSave(): void {
    if (saveTimer) {
      clearTimeout(saveTimer)
      saveTimer = null
    }
    if (!savePending) return
    savePending = false
    onSave()
  }

  $effect(() => {
    return () => flushSave()
  })

  /** Cancel path: drop the pending debounced save so the unmount flush cannot re-save the
   *  discarded draft AFTER the parent restored its snapshot. */
  export function discardPending(): void {
    if (saveTimer) {
      clearTimeout(saveTimer)
      saveTimer = null
    }
    savePending = false
  }

  function parseSegments(tpl: string): { fields: string[]; texts: string[] } {
    const fields: string[] = []
    const texts: string[] = []
    let lastEnd = 0
    for (const m of tpl.matchAll(/\{(\w+)\}/g)) {
      texts.push(tpl.slice(lastEnd, m.index))
      fields.push(m[1])
      lastEnd = m.index + m[0].length
    }
    texts.push(tpl.slice(lastEnd))
    return { fields, texts }
  }

  let parsed = $derived.by(() => parseSegments(templateInput))
  let fields = $derived(parsed.fields)
  let texts = $derived(parsed.texts)

  let dragFieldIdx = $state<number | null>(null)
  let dragFromPalette = $state<string | null>(null)
  let dragOverIdx = $state<number | null>(null)

  function rebuild(nextFields: string[], nextTexts: string[], debounce = false): void {
    templateInput = nextTexts[0] + nextFields.map((f, i) => `{${f}}` + nextTexts[i + 1]).join('')
    savePending = true
    if (debounce) {
      if (saveTimer) clearTimeout(saveTimer)
      saveTimer = setTimeout(flushSave, 400)
    } else {
      flushSave()
    }
  }

  function updateText(i: number, value: string): void {
    const t = [...texts]
    t[i] = value
    rebuild([...fields], t, true)
  }

  function appendField(key: string): void {
    if (fields.includes(key)) return
    const f = [...fields]
    const t = [...texts]
    // Auto-insert the default separator when this lands right after another field.
    if (f.length > 0 && t[t.length - 1] === '') t[t.length - 1] = defaultSep
    f.push(key)
    t.push('')
    rebuild(f, t)
  }

  // Last caret position inside a text segment — a palette click inserts the field THERE instead
  // of appending. Remembered on focus/click/keyup (the palette click itself blurs the input).
  let insertAt = $state<{ text: number; caret: number } | null>(null)

  function rememberCaret(i: number, el: HTMLInputElement): void {
    insertAt = { text: i, caret: el.selectionStart ?? el.value.length }
  }

  function insertFieldAt(key: string, textIdx: number, caret: number): void {
    const f = [...fields]
    const t = [...texts]
    let before = t[textIdx].slice(0, caret)
    let after = t[textIdx].slice(caret)
    // Landing flush against a neighboring chip gets the default separator, mirroring append.
    if (before === '' && textIdx > 0) before = defaultSep
    if (after === '' && textIdx < f.length) after = defaultSep
    f.splice(textIdx, 0, key)
    t.splice(textIdx, 1, before, after)
    rebuild(f, t)
    // Chain further inserts right after the new chip.
    insertAt = { text: textIdx + 1, caret: 0 }
  }

  // Palette click: insert at the remembered caret when there is one, otherwise append.
  function addField(key: string): void {
    if (fields.includes(key)) return
    if (insertAt && insertAt.text >= 0 && insertAt.text < texts.length) {
      insertFieldAt(key, insertAt.text, Math.min(insertAt.caret, texts[insertAt.text].length))
    } else {
      appendField(key)
    }
  }

  function insertFieldBefore(key: string, index: number): void {
    if (fields.includes(key)) return
    const f = [...fields]
    const t = [...texts]
    f.splice(index, 0, key)
    // New text slot between the inserted chip and the one it was dropped on.
    t.splice(index + 1, 0, defaultSep)
    rebuild(f, t)
  }

  function removeField(i: number): void {
    const f = [...fields]
    const t = [...texts]
    f.splice(i, 1)
    const merged = (t[i] + t[i + 1])
      .replace(/\/{2,}/g, '/')
      .replace(/-{2,}/g, '-')
      .replace(/_{2,}/g, '_')
      .replace(/ {2,}/g, ' ')
    t.splice(i, 2, merged)
    // Don't leave a dangling separator at the edges.
    t[0] = t[0].replace(/^[/\-_\s]+/, '')
    t[t.length - 1] = t[t.length - 1].replace(/[/\-_\s]+$/, '')
    rebuild(f, t)
  }

  function moveField(from: number, to: number): void {
    if (from === to) return
    const f = [...fields]
    const [moved] = f.splice(from, 1)
    f.splice(from < to ? to - 1 : to, 0, moved)
    rebuild(f, [...texts])
  }

  // Chips are keyed by position, so a move re-creates the DOM node — re-focus it by field name.
  async function moveFieldByKeyboard(i: number, dir: -1 | 1): Promise<void> {
    if (dir === -1 ? i === 0 : i === fields.length - 1) return
    const key = fields[i]
    moveField(i, dir === -1 ? i - 1 : i + 2)
    await tick()
    trackEl?.querySelector<HTMLElement>(`[data-chip-field="${key}"]`)?.focus()
  }

  function onChipDrop(index: number): void {
    if (dragFromPalette) {
      insertFieldBefore(dragFromPalette, index)
    } else if (dragFieldIdx !== null) {
      moveField(dragFieldIdx, index)
    }
    resetDrag()
  }

  function onTrackDrop(): void {
    if (dragFromPalette) {
      appendField(dragFromPalette)
    } else if (dragFieldIdx !== null) {
      moveField(dragFieldIdx, fields.length)
    }
    resetDrag()
  }

  function resetDrag(): void {
    dragFieldIdx = null
    dragFromPalette = null
    dragOverIdx = null
  }

  // Branch templates drop a separator automatically when the field AFTER it renders empty
  // (renderBranchName collapses it) — surface that rule on the separator slots themselves.
  let separatorTitle = $derived(
    autoSeparators
      ? 'Editable text — click and type any separator; fields from below insert at the cursor. A separator is dropped automatically when the field after it renders empty (e.g. a task without {parentKey}).'
      : 'Editable text — click and type any separator; fields from below insert at the cursor',
  )
</script>

<div class="flex flex-col gap-1.5">
  <div class="flex items-center gap-3">
    <span class="text-sm text-text-secondary w-20 shrink-0">{label}</span>
    <!-- svelte-ignore a11y_no_static_element_interactions -->
    <div
      bind:this={trackEl}
      class="flex flex-wrap items-center gap-y-1 flex-1 min-h-8 px-1.5 py-1 border border-border rounded-md bg-bg-input"
      ondragover={(e) => {
        e.preventDefault()
        if (dragFromPalette || dragFieldIdx !== null) dragOverIdx = fields.length
      }}
      ondrop={onTrackDrop}
    >
      {#each fields as field, i (`${i}:${field}`)}
        <!-- Dashed = still empty (a slot you can fill), solid = holds a separator (even a space).
             Text is centered so a lone "/" sits visually between its neighboring chips. -->
        <input
          class="bg-bg border rounded-sm outline-none font-mono text-xs text-text-secondary px-1 py-0.5 min-w-4 cursor-text mx-0.5 text-center hover:border-accent-muted hover:text-text focus:border-solid focus:border-focus-ring {texts[
            i
          ] === ''
            ? 'border-dashed border-border'
            : 'border-solid border-border'}"
          style={`width: calc(${Math.max(1, texts[i].length + 1)}ch + 0.5rem)`}
          value={texts[i]}
          oninput={(e) => updateText(i, e.currentTarget.value)}
          onfocus={(e) => rememberCaret(i, e.currentTarget)}
          onclick={(e) => rememberCaret(i, e.currentTarget)}
          onkeyup={(e) => rememberCaret(i, e.currentTarget)}
          aria-label={`Text before {${field}}`}
          spellcheck="false"
          autocomplete="off"
          title={separatorTitle}
        />
        <span
          class="inline-flex items-center gap-0.5 px-1.5 py-0.5 rounded-sm text-xs font-mono cursor-grab select-none bg-accent-bg text-accent-text border active:cursor-grabbing hover:border-focus-ring {dragOverIdx ===
          i
            ? 'border-focus-ring'
            : 'border-accent-muted'}"
          draggable="true"
          ondragstart={() => {
            dragFieldIdx = i
            dragFromPalette = null
          }}
          ondragover={(e) => {
            e.preventDefault()
            e.stopPropagation()
            dragOverIdx = i
          }}
          ondrop={(e) => {
            e.stopPropagation()
            onChipDrop(i)
          }}
          ondragend={resetDrag}
          onkeydown={(e) => {
            if (e.key === 'ArrowLeft') {
              e.preventDefault()
              void moveFieldByKeyboard(i, -1)
            } else if (e.key === 'ArrowRight') {
              e.preventDefault()
              void moveFieldByKeyboard(i, 1)
            }
          }}
          role="button"
          tabindex="0"
          data-chip-field={field}
          aria-label={`{${field}} — arrow keys reorder`}
          title="Drag or use ←/→ to reorder · remove with ×"
        >
          {`{${field}}`}
          <button
            type="button"
            class="inline-flex items-center justify-center size-3.5 border-0 rounded-full bg-transparent text-text-muted text-sm leading-none cursor-pointer p-0 opacity-60 hover:opacity-100 hover:bg-danger-bg hover:text-danger-text"
            onclick={() => removeField(i)}
            aria-label={`Remove {${field}}`}>×</button
          >
        </span>
      {/each}
      <input
        class="flex-1 bg-bg border rounded-sm outline-none font-mono text-xs text-text-secondary px-1 py-0.5 min-w-4 cursor-text mx-0.5 placeholder:text-text-faint hover:border-accent-muted hover:text-text focus:border-solid focus:border-focus-ring {texts[
          texts.length - 1
        ] === ''
          ? 'border-dashed border-border'
          : 'border-solid border-border text-center'}"
        style={fields.length === 0 && texts[texts.length - 1] === ''
          ? undefined
          : `flex: 0 1 auto; width: calc(${Math.max(1, texts[texts.length - 1].length + 1)}ch + 0.5rem)`}
        value={texts[texts.length - 1]}
        oninput={(e) => updateText(texts.length - 1, e.currentTarget.value)}
        onfocus={(e) => rememberCaret(texts.length - 1, e.currentTarget)}
        onclick={(e) => rememberCaret(texts.length - 1, e.currentTarget)}
        onkeyup={(e) => rememberCaret(texts.length - 1, e.currentTarget)}
        aria-label="Template text"
        placeholder={fields.length === 0 && texts[texts.length - 1] === ''
          ? 'Type text or click fields below'
          : undefined}
        spellcheck="false"
        autocomplete="off"
        title={separatorTitle}
      />
    </div>
  </div>

  {#if autoSeparators}
    <p class="text-2xs text-text-faint m-0 pl-23 leading-4">
      A separator is dropped automatically when the field after it renders empty — e.g. a task
      without {'{parentKey}'} produces no double slash.
    </p>
  {/if}

  <div class="flex items-start gap-3">
    <span class="text-sm text-text-secondary w-20 shrink-0 pt-1">Available fields</span>
    <div class="flex-1 flex flex-wrap items-center gap-1">
      {#each placeholders as ph (ph.key)}
        {@const used = fields.includes(ph.key)}
        <button
          type="button"
          class="text-xs px-1.5 py-0.5 border border-border rounded-sm bg-bg-input text-text-secondary font-mono cursor-pointer hover:bg-accent-bg hover:border-accent-muted hover:text-accent-text"
          class:opacity-35={used}
          class:!cursor-default={used}
          title={ph.description +
            ' (e.g. ' +
            ph.example +
            ') — click to insert at the cursor, drag to drop anywhere'}
          draggable="true"
          ondragstart={() => {
            dragFromPalette = ph.key
            dragFieldIdx = null
          }}
          ondragend={resetDrag}
          onclick={() => addField(ph.key)}
        >
          {`{${ph.key}}`}
        </button>
      {/each}
    </div>
  </div>
</div>
