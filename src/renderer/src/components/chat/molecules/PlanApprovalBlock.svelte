<script lang="ts">
  import type { Snippet } from 'svelte'
  import { ClipboardCheck } from '@lucide/svelte'
  import AttentionBanner from './AttentionBanner.svelte'
  import Chip from '../atoms/Chip.svelte'
  import TypingDots from '../atoms/TypingDots.svelte'
  import type { PermissionMode } from '../../../../../main/sdkAgents/types'

  export interface AllowedPrompt {
    tool: string
    prompt: string
  }

  type ApprovalMode = Exclude<PermissionMode, 'plan'>
  type Status = 'waiting' | 'submitting' | 'approved' | 'rejected'

  interface Props {
    title?: string
    status?: Status
    /** Rendered plan body (markdown or any content). Presentational only — caller handles markdown rendering. */
    plan: Snippet
    allowedPrompts?: AllowedPrompt[]
    /** Previous rejection feedback to render in the resolved view. */
    feedback?: string
    approvalMode?: ApprovalMode
    onapprove?: (mode: ApprovalMode) => void
    onreject?: (feedback?: string) => void
  }

  let {
    title = 'Plan ready for approval',
    status = 'waiting',
    plan,
    allowedPrompts = [],
    feedback: initialFeedback = '',
    approvalMode,
    onapprove,
    onreject,
  }: Props = $props()

  let showFeedback = $state(false)
  let feedbackText = $state(initialFeedback)

  const MAX_PROMPTS_BEFORE_COLLAPSE = 5
  let promptsExpanded = $state(false)
  let visiblePrompts = $derived.by(() => {
    if (promptsExpanded || allowedPrompts.length <= MAX_PROMPTS_BEFORE_COLLAPSE) {
      return allowedPrompts
    }
    return allowedPrompts.slice(0, MAX_PROMPTS_BEFORE_COLLAPSE)
  })
  let hiddenCount = $derived(
    allowedPrompts.length > MAX_PROMPTS_BEFORE_COLLAPSE
      ? allowedPrompts.length - MAX_PROMPTS_BEFORE_COLLAPSE
      : 0,
  )

  let readonly = $derived(status !== 'waiting')
  const APPROVAL_ACTIONS: { mode: ApprovalMode; label: string; variant: 'primary' | 'ghost' }[] = [
    { mode: 'bypassPermissions', label: 'Bypass permissions', variant: 'primary' },
    { mode: 'acceptEdits', label: 'Auto-accept edits', variant: 'ghost' },
    { mode: 'default', label: 'Default approvals', variant: 'ghost' },
  ]

  let displayTitle = $derived.by(() => {
    if (status === 'approved') return 'Plan approved'
    if (status === 'rejected') return 'Plan rejected'
    if (status === 'submitting') return 'Submitting…'
    return title
  })

  // Map PlanApproval Status -> AttentionBanner Status.
  let bannerStatus = $derived.by<'waiting' | 'submitting' | 'resolved' | 'rejected'>(() => {
    if (status === 'approved') return 'resolved'
    return status
  })

  function labelForMode(mode: ApprovalMode | undefined): string {
    if (!mode) return 'the selected mode'
    if (mode === 'bypassPermissions') return 'bypass permissions'
    if (mode === 'acceptEdits') return 'auto-accept edits'
    return 'default approvals'
  }

  function approveWithMode(mode: ApprovalMode): void {
    if (readonly) return
    onapprove?.(mode)
  }

  function startReject(): void {
    if (readonly) return
    showFeedback = true
  }

  function submitReject(): void {
    if (readonly) return
    onreject?.(feedbackText.trim() || undefined)
    showFeedback = false
  }

  function cancelReject(): void {
    showFeedback = false
  }
</script>

<AttentionBanner title={displayTitle} icon={ClipboardCheck} status={bannerStatus} tone="accent">
  {#snippet description()}
    {#if status === 'approved'}
      The assistant will proceed with this plan using {labelForMode(approvalMode)}.
    {:else if status === 'rejected'}
      The assistant will revise the plan.
    {:else if status === 'submitting'}
      Submitting your decision…
    {:else}
      Review the plan below and choose how to proceed.
    {/if}
  {/snippet}

  {#snippet body()}
    <div class="plan-wrapper">
      <div class="plan-body">{@render plan()}</div>
    </div>

    {#if allowedPrompts.length > 0}
      <div class="permissions">
        <div class="permissions-label">Requested capabilities</div>
        <div class="prompt-grid">
          {#each visiblePrompts as p (p.tool + p.prompt)}
            <div class="prompt-row">
              <Chip variant="accent">{p.tool}</Chip>
              <span class="prompt-text">{p.prompt}</span>
            </div>
          {/each}
          {#if hiddenCount > 0}
            <button
              type="button"
              class="toggle-more"
              onclick={() => (promptsExpanded = !promptsExpanded)}
            >
              {promptsExpanded ? 'Show fewer' : `Show ${hiddenCount} more`}
            </button>
          {/if}
        </div>
      </div>
    {/if}

    {#if status === 'rejected' && initialFeedback}
      <div class="feedback-readonly">
        <div class="feedback-label">Your feedback</div>
        <div class="feedback-readonly-value">{initialFeedback}</div>
      </div>
    {/if}

    {#if showFeedback && !readonly}
      <div class="feedback-form">
        <label class="feedback-label" for="plan-feedback">What should change?</label>
        <textarea
          id="plan-feedback"
          class="feedback-input"
          bind:value={feedbackText}
          placeholder="Describe what you'd like the assistant to revise…"
          rows="3"
        ></textarea>
        <div class="feedback-actions">
          <button type="button" class="btn primary" onclick={submitReject}>Send feedback</button>
          <button type="button" class="btn ghost" onclick={cancelReject}>Back</button>
        </div>
      </div>
    {/if}
  {/snippet}

  {#snippet actions()}
    {#if status === 'waiting' && !showFeedback}
      <div class="approval-actions">
        {#each APPROVAL_ACTIONS as action (action.mode)}
          <button
            type="button"
            class={`btn ${action.variant}`}
            onclick={() => approveWithMode(action.mode)}
          >
            {action.label}
          </button>
        {/each}
      </div>
      <button type="button" class="btn ghost" onclick={startReject}>Request changes</button>
    {:else if status === 'submitting'}
      <span class="status-inline">
        <TypingDots label="Submitting" />
        <span>Submitting…</span>
      </span>
    {:else if status === 'approved'}
      <span class="status-inline status-success">Approved</span>
    {:else if status === 'rejected' && !showFeedback}
      <span class="status-inline status-error">Rejected</span>
    {/if}
  {/snippet}
</AttentionBanner>

<style>
  .plan-wrapper {
    border: 1px solid var(--c-border-subtle);
    border-radius: 6px;
    background: var(--c-bg);
    padding: 10px 12px;
    max-height: 300px;
    overflow-y: auto;
  }

  .plan-body {
    font-size: 12.5px;
    color: var(--c-text);
    line-height: 1.55;
  }

  .plan-body :global(p) {
    margin: 0 0 8px;
  }

  .plan-body :global(p:last-child) {
    margin-bottom: 0;
  }

  .plan-body :global(h1),
  .plan-body :global(h2),
  .plan-body :global(h3) {
    margin: 10px 0 6px;
    font-size: 13px;
    font-weight: 600;
  }

  .plan-body :global(ul),
  .plan-body :global(ol) {
    margin: 4px 0 8px;
    padding-left: 20px;
  }

  .plan-body :global(li) {
    margin-bottom: 2px;
  }

  .plan-body :global(code) {
    font-family: ui-monospace, SFMono-Regular, 'SF Mono', Menlo, Monaco, Consolas, monospace;
    font-size: 11.5px;
    background: var(--c-bg-elevated);
    padding: 1px 4px;
    border-radius: 3px;
  }

  .permissions {
    display: flex;
    flex-direction: column;
    gap: 6px;
  }

  .permissions-label,
  .feedback-label {
    font-size: 10.5px;
    text-transform: uppercase;
    letter-spacing: 0.05em;
    color: var(--c-text-muted);
    font-weight: 600;
  }

  .prompt-grid {
    display: flex;
    flex-direction: column;
    gap: 4px;
  }

  .prompt-row {
    display: flex;
    align-items: center;
    gap: 8px;
    font-size: 12px;
    color: var(--c-text-secondary);
  }

  .prompt-text {
    flex: 1;
    min-width: 0;
    line-height: 1.4;
  }

  .toggle-more {
    align-self: flex-start;
    background: transparent;
    border: none;
    color: var(--c-accent);
    font-size: 11.5px;
    cursor: pointer;
    padding: 2px 0;
    font-family: inherit;
  }

  .toggle-more:hover {
    color: var(--c-accent-text);
    text-decoration: underline;
  }

  .toggle-more:focus-visible {
    outline: none;
    box-shadow: 0 0 0 2px var(--c-focus-ring);
    border-radius: 2px;
  }

  .feedback-form,
  .feedback-readonly {
    display: flex;
    flex-direction: column;
    gap: 6px;
    padding: 10px 12px;
    border: 1px solid var(--c-border-subtle);
    border-radius: 6px;
    background: color-mix(in srgb, var(--c-danger) 5%, transparent);
  }

  .feedback-readonly {
    background: color-mix(in srgb, var(--c-danger) 8%, transparent);
  }

  .feedback-input {
    padding: 6px 10px;
    background: var(--c-bg);
    border: 1px solid var(--c-border);
    border-radius: 4px;
    color: var(--c-text);
    font-size: 12px;
    font-family: inherit;
    outline: none;
    resize: vertical;
  }

  .feedback-input:focus-visible {
    border-color: var(--c-focus-ring);
    box-shadow: 0 0 0 2px var(--c-focus-ring);
  }

  .feedback-readonly-value {
    padding: 6px 10px;
    background: var(--c-bg);
    border: 1px solid var(--c-border-subtle);
    border-radius: 4px;
    color: var(--c-text-secondary);
    font-size: 12px;
    line-height: 1.5;
    white-space: pre-wrap;
  }

  .feedback-actions {
    display: flex;
    gap: 8px;
  }

  .approval-actions {
    display: flex;
    flex-wrap: wrap;
    gap: 8px;
  }

  .btn {
    display: inline-flex;
    align-items: center;
    gap: 6px;
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
</style>
