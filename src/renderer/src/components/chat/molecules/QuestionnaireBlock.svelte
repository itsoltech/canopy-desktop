<script lang="ts">
  import { HelpCircle } from '@lucide/svelte'
  import AttentionBanner from './AttentionBanner.svelte'
  import QuestionBlock from './QuestionBlock.svelte'
  import type { QuestionAnswer, QuestionOptionSpec } from './QuestionBlock.svelte'
  import Divider from '../atoms/Divider.svelte'
  import Kbd from '../atoms/Kbd.svelte'
  import TypingDots from '../atoms/TypingDots.svelte'

  export interface Question {
    question: string
    header: string
    options: QuestionOptionSpec[]
    multiSelect?: boolean
  }

  type Status = 'waiting' | 'submitting' | 'resolved' | 'rejected'

  interface Props {
    questions: Question[]
    status?: Status
    title?: string
    /** Read-only answers snapshot (rendering past response in chat history). */
    answers?: Record<string, QuestionAnswer>
    onsubmit?: (answers: Record<string, QuestionAnswer>) => void
    oncancel?: () => void
  }

  let {
    questions,
    status = 'waiting',
    title = 'The assistant has a question',
    answers: initialAnswers,
    onsubmit,
    oncancel,
  }: Props = $props()

  let answers = $state<Record<string, QuestionAnswer>>(initialAnswers ?? {})

  let readonly = $derived(status === 'resolved' || status === 'rejected' || status === 'submitting')

  let allAnswered = $derived.by(() => {
    return questions.every((q) => {
      const a = answers[q.question]
      if (!a) return false
      const hasSelection = a.selected.length > 0
      if (!hasSelection) return false
      // If "Other" is the only selection, require some text.
      if (a.selected.length === 1 && a.selected[0] === 'Other') {
        return !!(a.other && a.other.trim().length > 0)
      }
      return true
    })
  })

  let displayTitle = $derived.by(() => {
    if (status === 'resolved') return 'Answered'
    if (status === 'rejected') return 'Skipped'
    if (questions.length > 1) return `${title} (${questions.length} questions)`
    return title
  })

  function handleAnswer(questionText: string, answer: QuestionAnswer): void {
    answers[questionText] = answer
  }

  function submit(): void {
    if (!allAnswered || readonly) return
    onsubmit?.({ ...answers })
  }

  function cancel(): void {
    if (readonly) return
    oncancel?.()
  }
</script>

<AttentionBanner
  title={displayTitle}
  icon={HelpCircle}
  {status}
  tone="warning"
  collapsible={status === 'resolved' || status === 'rejected'}
  defaultOpen={status !== 'resolved' && status !== 'rejected'}
>
  {#snippet description()}
    {#if status === 'resolved'}
      Your response has been submitted.
    {:else if status === 'rejected'}
      This question was cancelled.
    {:else if status === 'submitting'}
      Submitting your answer…
    {:else}
      Please answer the {questions.length > 1 ? 'questions' : 'question'} below to continue.
    {/if}
  {/snippet}

  {#snippet body()}
    <div class="questions">
      {#each questions as q, i (q.question)}
        {#if i > 0}
          <Divider />
        {/if}
        <QuestionBlock
          question={q.question}
          header={q.header}
          options={q.options}
          multiSelect={q.multiSelect}
          disabled={readonly}
          initialSelected={answers[q.question]?.selected ?? []}
          initialOther={answers[q.question]?.other ?? ''}
          initialNotes={answers[q.question]?.notes ?? ''}
          onanswer={(answer) => handleAnswer(q.question, answer)}
        />
      {/each}
    </div>
  {/snippet}

  {#snippet actions()}
    {#if status === 'waiting'}
      <button
        type="button"
        class="btn primary"
        onclick={submit}
        disabled={!allAnswered}
        aria-label="Submit answers"
      >
        <span>Submit</span>
        <span class="kbd-pair">
          <Kbd>⌘</Kbd>
          <Kbd>↵</Kbd>
        </span>
      </button>
      <button type="button" class="btn ghost" onclick={cancel}>Cancel</button>
    {:else if status === 'submitting'}
      <span class="status-inline">
        <TypingDots label="Submitting" />
        <span>Submitting…</span>
      </span>
    {:else if status === 'resolved'}
      <span class="status-inline status-success">Submitted</span>
    {:else if status === 'rejected'}
      <span class="status-inline status-error">Cancelled</span>
    {/if}
  {/snippet}
</AttentionBanner>

<style>
  .questions {
    display: flex;
    flex-direction: column;
    gap: 14px;
  }

  .btn {
    display: inline-flex;
    align-items: center;
    gap: 8px;
    padding: 6px 12px;
    border-radius: 6px;
    border: 1px solid transparent;
    background: transparent;
    font-size: 12.5px;
    font-family: inherit;
    cursor: pointer;
    outline: none;
    transition:
      background 0.1s,
      border-color 0.1s,
      color 0.1s;
  }

  .btn:focus-visible {
    box-shadow: 0 0 0 2px var(--c-focus-ring);
  }

  .btn:disabled {
    opacity: 0.45;
    cursor: default;
  }

  .btn.primary {
    background: var(--c-accent);
    color: var(--c-bg);
    font-weight: 600;
  }

  .btn.primary:hover:not(:disabled) {
    background: var(--c-accent-text);
  }

  .btn.ghost {
    color: var(--c-text-secondary);
    border-color: var(--c-border-subtle);
  }

  .btn.ghost:hover:not(:disabled) {
    background: var(--c-hover);
    color: var(--c-text);
    border-color: var(--c-border);
  }

  .status-inline {
    display: inline-flex;
    align-items: center;
    gap: 8px;
    font-size: 12px;
    color: var(--c-text-muted);
  }

  .status-inline.status-success {
    color: var(--c-success);
  }

  .status-inline.status-error {
    color: var(--c-danger);
  }

  .kbd-pair {
    display: inline-flex;
    align-items: center;
    gap: 2px;
  }
</style>
