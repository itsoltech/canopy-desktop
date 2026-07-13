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

  // Default separator inserted when a field is appended right after another one.
  let defaultSep = $derived(autoSeparators ? '/' : ' ')

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

  function rebuild(nextFields: string[], nextTexts: string[]): void {
    templateInput = nextTexts[0] + nextFields.map((f, i) => `{${f}}` + nextTexts[i + 1]).join('')
    onSave()
  }

  function updateText(i: number, value: string): void {
    const t = [...texts]
    t[i] = value
    rebuild([...fields], t)
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
</script>

<div class="flex flex-col gap-1.5">
  <div class="flex items-center gap-3">
    <span class="text-sm text-text-secondary w-20 shrink-0">{label}</span>
    <!-- svelte-ignore a11y_no_static_element_interactions -->
    <div
      class="flex flex-wrap items-center gap-y-1 flex-1 min-h-8 px-1.5 py-1 border border-border rounded-md bg-bg-input"
      ondragover={(e) => {
        e.preventDefault()
        if (dragFromPalette || dragFieldIdx !== null) dragOverIdx = fields.length
      }}
      ondrop={onTrackDrop}
    >
      {#each fields as field, i (`${i}:${field}`)}
        <input
          class="bg-bg border border-dashed border-border rounded-sm outline-none font-mono text-xs text-text-secondary px-1 py-0.5 min-w-4 cursor-text mx-0.5 hover:border-accent-muted hover:text-text focus:border-solid focus:border-focus-ring"
          style={`width: calc(${Math.max(1, texts[i].length + 1)}ch + 0.5rem)`}
          value={texts[i]}
          oninput={(e) => updateText(i, e.currentTarget.value)}
          aria-label={`Text before {${field}}`}
          spellcheck="false"
          autocomplete="off"
          title="Editable text — click and type any separator"
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
          role="listitem"
          title="Drag to reorder · remove with ×"
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
        class="flex-1 bg-bg border border-dashed border-border rounded-sm outline-none font-mono text-xs text-text-secondary px-1 py-0.5 min-w-4 cursor-text mx-0.5 placeholder:text-text-faint hover:border-accent-muted hover:text-text focus:border-solid focus:border-focus-ring"
        style={fields.length === 0 && texts[texts.length - 1] === ''
          ? undefined
          : `flex: 0 1 auto; width: calc(${Math.max(1, texts[texts.length - 1].length + 1)}ch + 0.5rem)`}
        value={texts[texts.length - 1]}
        oninput={(e) => updateText(texts.length - 1, e.currentTarget.value)}
        aria-label="Template text"
        placeholder={fields.length === 0 && texts[texts.length - 1] === ''
          ? 'Type text or click fields below'
          : undefined}
        spellcheck="false"
        autocomplete="off"
        title="Editable text — click and type any separator"
      />
    </div>
  </div>

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
          title={ph.description + ' (e.g. ' + ph.example + ')'}
          draggable="true"
          ondragstart={() => {
            dragFromPalette = ph.key
            dragFieldIdx = null
          }}
          ondragend={resetDrag}
          onclick={() => appendField(ph.key)}
        >
          {`{${ph.key}}`}
        </button>
      {/each}
    </div>
  </div>
</div>
