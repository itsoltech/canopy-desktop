<script lang="ts">
  import type { Component, Snippet } from 'svelte'

  type Tone = 'warning' | 'accent' | 'danger'
  type Status = 'waiting' | 'submitting' | 'resolved' | 'rejected'

  interface Props {
    title: string
    /** Lucide icon component rendered in the header badge. */
    icon?: Component<{ size?: number; strokeWidth?: number }>
    /** Drives left-rail color + pulse. */
    status?: Status
    /** Base accent for rail + icon. `resolved`/`rejected` override to success/danger. */
    tone?: Tone
    description?: Snippet
    body?: Snippet
    actions?: Snippet
  }

  let {
    title,
    icon: Icon,
    status = 'waiting',
    tone = 'warning',
    description,
    body,
    actions,
  }: Props = $props()

  let effectiveTone = $derived.by<Tone | 'success'>(() => {
    if (status === 'resolved') return 'success'
    if (status === 'rejected') return 'danger'
    return tone
  })
</script>

<section class="banner" data-tone={effectiveTone} data-status={status}>
  <header class="head">
    {#if Icon}
      <span class="icon" aria-hidden="true">
        <Icon size={15} strokeWidth={2} />
      </span>
    {/if}
    <span class="title">{title}</span>
  </header>

  {#if description}
    <div class="description">{@render description()}</div>
  {/if}

  {#if body}
    <div class="body">{@render body()}</div>
  {/if}

  {#if actions}
    <div class="actions">{@render actions()}</div>
  {/if}
</section>

<style>
  .banner {
    position: relative;
    margin: 8px 0;
    border-radius: 8px;
    border: 1px solid color-mix(in srgb, var(--tone-color) 28%, transparent);
    background: color-mix(in srgb, var(--tone-color) 6%, transparent);
    overflow: hidden;
    padding: 10px 14px 12px 16px;
    display: flex;
    flex-direction: column;
    gap: 8px;
    --tone-color: var(--c-warning);
  }

  .banner[data-tone='accent'] {
    --tone-color: var(--c-accent);
  }

  .banner[data-tone='warning'] {
    --tone-color: var(--c-warning);
  }

  .banner[data-tone='danger'] {
    --tone-color: var(--c-danger);
  }

  .banner[data-tone='success'] {
    --tone-color: var(--c-success);
  }

  /* Left rail — mirrors SubAgentRun. 3px bar, pulses while waiting. */
  .banner::before {
    content: '';
    position: absolute;
    top: 0;
    bottom: 0;
    left: 0;
    width: 3px;
    background: var(--tone-color);
    opacity: 0.6;
  }

  .banner[data-status='waiting']::before,
  .banner[data-status='submitting']::before {
    animation: rail-pulse 1.6s ease-in-out infinite;
  }

  @keyframes rail-pulse {
    0%,
    100% {
      opacity: 0.55;
    }
    50% {
      opacity: 1;
    }
  }

  @media (prefers-reduced-motion: reduce) {
    .banner::before {
      animation: none !important;
    }
  }

  .head {
    display: flex;
    align-items: center;
    gap: 8px;
    color: var(--tone-color);
    font-size: 12.5px;
    font-weight: 600;
    letter-spacing: 0.01em;
  }

  .icon {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    width: 22px;
    height: 22px;
    border-radius: 6px;
    background: color-mix(in srgb, var(--tone-color) 15%, transparent);
    color: var(--tone-color);
    flex-shrink: 0;
  }

  .title {
    color: var(--c-text);
  }

  .description {
    font-size: 12.5px;
    color: var(--c-text-secondary);
    line-height: 1.5;
  }

  .body {
    display: flex;
    flex-direction: column;
    gap: 8px;
  }

  .actions {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 8px;
    padding-top: 2px;
  }
</style>
