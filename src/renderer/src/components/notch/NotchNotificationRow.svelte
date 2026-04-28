<script lang="ts">
  import { ShieldAlert, CircleDot, Loader, AlertTriangle, ChevronRight } from 'lucide-svelte'

  interface Props {
    session: {
      ptySessionId: string
      windowId: number
      workspaceName: string
      branch: string | null
      status:
        | 'idle'
        | 'thinking'
        | 'toolCalling'
        | 'compacting'
        | 'waitingPermission'
        | 'error'
        | 'ended'
      toolName?: string
      detail?: string
      title?: string
    }
    highlight?: boolean
    onclick: () => void
  }

  let { session, highlight = false, onclick }: Props = $props()

  const config = $derived(
    {
      idle: {
        color: 'var(--color-status-idle)',
        label: highlight ? 'Finished' : 'Idle',
      },
      thinking: { color: 'var(--color-status-working)', label: 'Thinking...' },
      toolCalling: {
        color: 'var(--color-status-working)',
        label: session.toolName ?? 'Tool call',
      },
      compacting: { color: 'var(--color-status-compacting)', label: 'Compacting...' },
      waitingPermission: {
        color: 'var(--color-status-permission)',
        label: session.detail ?? 'Permission needed',
      },
      error: {
        color: 'var(--color-status-error)',
        label: session.detail ?? 'Error',
      },
      ended: { color: 'var(--color-status-ended)', label: 'Ended' },
    }[session.status],
  )

  const isActive = $derived(
    session.status === 'thinking' ||
      session.status === 'toolCalling' ||
      session.status === 'compacting',
  )
</script>

<button
  class="notch-row flex items-center gap-2.5 h-12 box-border px-3 py-2 w-full border-0 bg-transparent rounded-2xl last:rounded-b-notch-row cursor-pointer text-left text-notch-text transition duration-base motion-reduce:transition-none hover:bg-notch-row-hover active:scale-98 motion-reduce:active:scale-100"
  class:animate-peek-pulse={highlight}
  class:motion-reduce:animate-none={highlight}
  {onclick}
>
  <span
    class="flex-shrink-0 flex items-center justify-center w-7 h-7 bg-notch-icon-bg rounded-xl"
    style:color={config.color}
  >
    {#if session.status === 'waitingPermission'}
      <ShieldAlert size={15} />
    {:else if isActive}
      <span class="flex animate-spin-slow motion-reduce:animate-none"><Loader size={15} /></span>
    {:else if session.status === 'error'}
      <AlertTriangle size={15} />
    {:else}
      <CircleDot size={15} />
    {/if}
  </span>

  <span class="flex flex-col gap-0.5 min-w-0 flex-1 overflow-hidden">
    <span class="text-md font-medium truncate text-notch-text">
      {session.workspaceName}
      {#if session.branch}
        <span class="text-notch-text-dim font-normal ml-1.5 text-sm">{session.branch}</span>
      {/if}
      {#if session.title}
        <span class="text-notch-text-dim font-normal ml-1.5 text-sm">· {session.title}</span>
      {/if}
    </span>
    <span
      class="text-xs whitespace-nowrap overflow-hidden text-ellipsis opacity-85"
      style:color={config.color}>{config.label}</span
    >
  </span>

  <span class="flex-shrink-0 flex items-center text-notch-chevron">
    <ChevronRight size={14} />
  </span>
</button>
