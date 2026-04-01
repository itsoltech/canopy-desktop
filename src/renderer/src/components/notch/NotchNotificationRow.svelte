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
      idle: { color: '#4ade80', label: highlight ? 'Finished' : 'Idle' },
      thinking: { color: '#f59e0b', label: 'Thinking...' },
      toolCalling: { color: '#f59e0b', label: session.toolName ?? 'Tool call' },
      compacting: { color: '#60a5fa', label: 'Compacting...' },
      waitingPermission: { color: '#f87171', label: session.detail ?? 'Permission needed' },
      error: { color: '#f87171', label: session.detail ?? 'Error' },
      ended: { color: '#666', label: 'Ended' },
    }[session.status],
  )

  const isActive = $derived(
    session.status === 'thinking' ||
      session.status === 'toolCalling' ||
      session.status === 'compacting',
  )
</script>

<button class="row" class:highlight {onclick}>
  <span class="icon" style:color={config.color}>
    {#if session.status === 'waitingPermission'}
      <ShieldAlert size={15} />
    {:else if isActive}
      <span class="spin"><Loader size={15} /></span>
    {:else if session.status === 'error'}
      <AlertTriangle size={15} />
    {:else}
      <CircleDot size={15} />
    {/if}
  </span>

  <span class="info">
    <span class="name">
      {session.workspaceName}
      {#if session.branch}
        <span class="branch">{session.branch}</span>
      {/if}
      {#if session.title}
        <span class="title">{session.title}</span>
      {/if}
    </span>
    <span class="status-label" style:color={config.color}>{config.label}</span>
  </span>

  <span class="chevron">
    <ChevronRight size={14} color="rgba(255,255,255,0.2)" />
  </span>
</button>

<style>
  .row {
    display: flex;
    align-items: center;
    gap: 10px;
    height: 48px;
    box-sizing: border-box;
    padding: 8px 12px;
    margin: 0 6px;
    width: calc(100% - 12px);
    border: none;
    background: transparent;
    border-radius: 10px;
    cursor: pointer;
    text-align: left;
    color: rgba(255, 255, 255, 0.9); /* fixed: notch overlay always on black bg */
    transition:
      background 0.15s ease,
      transform 0.15s cubic-bezier(0.32, 0.72, 0, 1);
  }

  .row.highlight {
    animation: peek-pulse 2s ease-out;
  }

  @keyframes peek-pulse {
    0% {
      background: rgba(255, 255, 255, 0.1);
    }
    100% {
      background: transparent;
    }
  }

  .row:hover {
    background: rgba(255, 255, 255, 0.08);
  }

  .row:active {
    transform: scale(0.98);
  }

  .icon {
    flex-shrink: 0;
    display: flex;
    align-items: center;
    justify-content: center;
    width: 28px;
    height: 28px;
    background: rgba(255, 255, 255, 0.06);
    border-radius: 8px;
  }

  .spin {
    display: flex;
    animation: rotate 1.5s linear infinite;
  }

  @keyframes rotate {
    from {
      transform: rotate(0deg);
    }
    to {
      transform: rotate(360deg);
    }
  }

  .info {
    display: flex;
    flex-direction: column;
    gap: 2px;
    min-width: 0;
    flex: 1;
    overflow: hidden;
  }

  .name {
    font-size: 13px;
    font-weight: 500;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
    color: rgba(255, 255, 255, 0.9);
  }

  .branch {
    color: rgba(255, 255, 255, 0.35);
    font-weight: 400;
    margin-left: 6px;
    font-size: 12px;
  }

  .title {
    color: rgba(255, 255, 255, 0.35);
    font-weight: 400;
    margin-left: 6px;
    font-size: 12px;
  }

  .title::before {
    content: '\00b7\00a0';
  }

  .status-label {
    font-size: 11px;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
    opacity: 0.85;
  }

  .chevron {
    flex-shrink: 0;
    display: flex;
    align-items: center;
  }

  @media (prefers-reduced-motion: reduce) {
    .spin {
      animation: none;
    }
    .row.highlight {
      animation: none;
    }
  }
</style>
