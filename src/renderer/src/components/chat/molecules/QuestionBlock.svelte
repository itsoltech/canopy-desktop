<script lang="ts">
  import { SvelteSet } from 'svelte/reactivity'
  import Chip from '../atoms/Chip.svelte'
  import QuestionOption from './QuestionOption.svelte'

  export interface QuestionOptionSpec {
    label: string
    description?: string
    /** Multi-line monospace preview text. Presence enables side-by-side layout for the parent question. */
    preview?: string
  }

  export interface QuestionAnswer {
    /** Labels the user selected. For single-select there is at most one. */
    selected: string[]
    /** Freetext from the auto-injected "Other" option, if chosen. */
    other?: string
    /** Annotation text (only shown when a previewed option is selected). */
    notes?: string
  }

  interface Props {
    /** Short chip label (≤12 chars per AskUserQuestion spec). */
    header: string
    question: string
    options: QuestionOptionSpec[]
    multiSelect?: boolean
    disabled?: boolean
    /** Hide the auto-injected "Other" freetext option. */
    hideOther?: boolean
    /** Initial selection (option labels). */
    initialSelected?: string[]
    initialOther?: string
    initialNotes?: string
    onanswer?: (answer: QuestionAnswer) => void
  }

  let {
    header,
    question,
    options,
    multiSelect = false,
    disabled = false,
    hideOther = false,
    initialSelected = [],
    initialOther = '',
    initialNotes = '',
    onanswer,
  }: Props = $props()

  const OTHER_LABEL = 'Other'

  const selected = new SvelteSet<string>(initialSelected)
  let otherText = $state(initialOther)
  let notesText = $state(initialNotes)

  let hasAnyPreview = $derived(options.some((o) => o.preview && o.preview.length > 0))

  // Track which selected option surfaces its preview on the right pane.
  // For single-select: the one currently chosen. For multi-select: the most
  // recently clicked previewed option.
  let previewFocusLabel = $state<string | null>(null)
  let currentPreview = $derived.by(() => {
    if (!hasAnyPreview) return null
    const label = previewFocusLabel
    if (!label) return null
    const opt = options.find((o) => o.label === label)
    return opt?.preview ?? null
  })

  let selectedHasPreview = $derived.by(() => {
    for (const label of selected) {
      const opt = options.find((o) => o.label === label)
      if (opt?.preview) return true
    }
    return false
  })

  function selectOption(label: string): void {
    if (disabled) return
    const hasPreview = options.find((o) => o.label === label)?.preview

    if (multiSelect) {
      if (selected.has(label)) {
        selected.delete(label)
      } else {
        selected.add(label)
      }
      if (hasPreview) previewFocusLabel = label
    } else {
      selected.clear()
      selected.add(label)
      previewFocusLabel = hasPreview ? label : null
    }
    emit()
  }

  function selectOther(): void {
    if (disabled) return
    if (multiSelect) {
      if (selected.has(OTHER_LABEL)) {
        selected.delete(OTHER_LABEL)
      } else {
        selected.add(OTHER_LABEL)
      }
    } else {
      selected.clear()
      selected.add(OTHER_LABEL)
      previewFocusLabel = null
    }
    emit()
  }

  function emit(): void {
    onanswer?.({
      selected: [...selected],
      other: selected.has(OTHER_LABEL) ? otherText : undefined,
      notes: notesText || undefined,
    })
  }
</script>

<section class="question-block" class:has-preview={hasAnyPreview} aria-disabled={disabled}>
  <header class="head">
    <Chip variant="accent">{header}</Chip>
    <h3 class="question">{question}</h3>
  </header>

  <div class="layout" class:split={hasAnyPreview}>
    <div class="options" role={multiSelect ? 'group' : 'radiogroup'} aria-label={question}>
      {#each options as option (option.label)}
        <QuestionOption
          label={option.label}
          description={option.description}
          selected={selected.has(option.label)}
          {multiSelect}
          {disabled}
          hasPreview={!!option.preview}
          onselect={() => selectOption(option.label)}
        />
      {/each}

      {#if !hideOther}
        <div class="other-row" class:selected={selected.has(OTHER_LABEL)}>
          <QuestionOption
            label={OTHER_LABEL}
            description={multiSelect ? 'Add your own (also combinable)' : 'Write your own answer'}
            selected={selected.has(OTHER_LABEL)}
            {multiSelect}
            {disabled}
            onselect={selectOther}
          />
          {#if selected.has(OTHER_LABEL)}
            <input
              class="other-input"
              type="text"
              placeholder="Type your answer…"
              bind:value={otherText}
              oninput={emit}
              {disabled}
              aria-label="Other answer"
            />
          {/if}
        </div>
      {/if}
    </div>

    {#if hasAnyPreview}
      <aside class="preview-pane" aria-label="Preview">
        {#if currentPreview}
          <pre class="preview-code">{currentPreview}</pre>
        {:else}
          <div class="preview-empty">Select an option to see the preview.</div>
        {/if}
      </aside>
    {/if}
  </div>

  {#if selectedHasPreview && !disabled}
    <div class="notes">
      <label class="notes-label" for="notes-{header}">Notes (optional)</label>
      <textarea
        id="notes-{header}"
        class="notes-input"
        placeholder="Any comments on this choice…"
        bind:value={notesText}
        oninput={emit}
        rows="2"
      ></textarea>
    </div>
  {:else if notesText && disabled}
    <div class="notes notes-readonly">
      <span class="notes-label">Notes</span>
      <div class="notes-readonly-value">{notesText}</div>
    </div>
  {/if}
</section>

<style>
  .question-block {
    display: flex;
    flex-direction: column;
    gap: 10px;
    container-type: inline-size;
  }

  .head {
    display: flex;
    align-items: center;
    gap: 10px;
    flex-wrap: wrap;
  }

  .question {
    margin: 0;
    font-size: 13px;
    font-weight: 500;
    color: var(--c-text);
    line-height: 1.45;
    flex: 1;
    min-width: 0;
  }

  .layout {
    display: flex;
    flex-direction: column;
    gap: 10px;
  }

  .layout.split {
    display: grid;
    grid-template-columns: minmax(0, 1fr) minmax(0, 1fr);
    gap: 12px;
  }

  @container (max-width: 520px) {
    .layout.split {
      grid-template-columns: 1fr;
    }
  }

  .options {
    display: flex;
    flex-direction: column;
    gap: 6px;
  }

  .other-row {
    display: flex;
    flex-direction: column;
    gap: 6px;
  }

  .other-input {
    margin-left: 34px;
    padding: 6px 10px;
    background: var(--c-bg);
    border: 1px solid var(--c-border);
    border-radius: 4px;
    color: var(--c-text);
    font-size: 12px;
    outline: none;
    font-family: inherit;
  }

  .other-input:focus-visible {
    border-color: var(--c-focus-ring);
    box-shadow: 0 0 0 2px var(--c-focus-ring);
  }

  .other-input:disabled {
    opacity: 0.6;
    cursor: default;
  }

  .preview-pane {
    display: flex;
    flex-direction: column;
    border: 1px solid var(--c-border-subtle);
    border-radius: 6px;
    background: var(--c-bg);
    overflow: hidden;
    min-height: 120px;
  }

  .preview-code {
    margin: 0;
    padding: 10px 12px;
    font-family: ui-monospace, SFMono-Regular, 'SF Mono', Menlo, Monaco, Consolas, monospace;
    font-size: 11px;
    line-height: 1.5;
    color: var(--c-text);
    white-space: pre;
    overflow: auto;
    flex: 1;
    -webkit-user-select: text;
    user-select: text;
  }

  .preview-empty {
    padding: 16px 12px;
    font-size: 11.5px;
    color: var(--c-text-muted);
    font-style: italic;
    text-align: center;
  }

  .notes {
    display: flex;
    flex-direction: column;
    gap: 4px;
    padding-top: 2px;
  }

  .notes-label {
    font-size: 10.5px;
    color: var(--c-text-muted);
    text-transform: uppercase;
    letter-spacing: 0.05em;
    font-weight: 600;
  }

  .notes-input {
    padding: 6px 10px;
    background: var(--c-bg);
    border: 1px solid var(--c-border);
    border-radius: 4px;
    color: var(--c-text);
    font-size: 12px;
    font-family: inherit;
    outline: none;
    resize: vertical;
    min-height: 48px;
  }

  .notes-input:focus-visible {
    border-color: var(--c-focus-ring);
    box-shadow: 0 0 0 2px var(--c-focus-ring);
  }

  .notes-readonly-value {
    padding: 6px 10px;
    background: var(--c-bg);
    border: 1px solid var(--c-border-subtle);
    border-radius: 4px;
    color: var(--c-text-secondary);
    font-size: 12px;
    line-height: 1.5;
    white-space: pre-wrap;
  }
</style>
