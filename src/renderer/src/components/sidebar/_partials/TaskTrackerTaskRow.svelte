<script lang="ts">
  import { Unlink } from '@lucide/svelte'
  import { statusChipClass } from '../../../lib/taskTracker/statusChip'
  import type { PanelTaskContext } from '../../../lib/stores/taskTracker.svelte'

  interface Props {
    task: PanelTaskContext
    fromBranch: boolean
    onOpen: () => void
    onUnlink: () => void
    class?: string
  }

  let { task, fromBranch, onOpen, onUnlink, class: className = '' }: Props = $props()
</script>

<!-- svelte-ignore a11y_click_events_have_key_events -->
<div
  class={`group flex items-center gap-2.5 w-full h-7 px-3 bg-transparent text-sm cursor-pointer text-left transition-colors duration-fast hover:bg-hover ${className}`}
  role="button"
  tabindex="0"
  onclick={onOpen}
  onkeydown={(event) => {
    if (event.key === 'Enter' || event.key === ' ') {
      event.preventDefault()
      onOpen()
    }
  }}
  title={task.summary
    ? `${task.taskKey} — ${task.summary}\nOpen the task panel (status, comments)`
    : 'Open the task panel (status, comments)'}
>
  {#if task.typeIcon}
    <img
      src={task.typeIcon}
      alt={task.typeName ?? task.type ?? 'task type'}
      title={task.typeName ?? task.type}
      class="size-3.5 shrink-0 rounded-sm"
    />
  {/if}
  <span
    class="text-xs font-semibold flex-shrink-0 {task.missing
      ? 'text-warning-text line-through'
      : 'text-accent-text'}">{task.taskKey}</span
  >
  {#if task.missing}
    <span class="px-1.5 py-px rounded-md text-2xs flex-shrink-0 bg-warning/15 text-warning-text"
      >not found</span
    >
  {/if}
  <span
    class="flex-1 min-w-0 overflow-hidden text-ellipsis whitespace-nowrap text-xs text-text-muted"
    >{task.summary ?? ''}</span
  >
  <!-- VS Code-style swap: the status chip yields its slot to unlink on hover/focus. -->
  {#if task.status}
    <span
      class="px-1.5 py-px rounded-md text-2xs flex-shrink-0 group-hover:hidden group-focus-within:hidden {statusChipClass(
        task.statusCategory,
      )}">{task.status}</span
    >
  {/if}
  <button
    class="hidden group-hover:flex group-focus-within:flex items-center justify-center size-5 rounded-md border-0 bg-transparent text-text-faint p-0 shrink-0 enabled:cursor-pointer enabled:hover:bg-danger-bg enabled:hover:text-danger-text disabled:cursor-not-allowed disabled:opacity-40"
    onclick={(event) => {
      event.stopPropagation()
      onUnlink()
    }}
    disabled={fromBranch}
    aria-label="Unlink task"
    title={fromBranch
      ? 'This task key is part of the branch name — the link comes from the branch and cannot be removed'
      : 'Unlink this task from the worktree'}
  >
    <Unlink size={12} />
  </button>
</div>
