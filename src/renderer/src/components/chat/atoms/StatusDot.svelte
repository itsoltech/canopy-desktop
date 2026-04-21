<script lang="ts">
  type Status = 'idle' | 'thinking' | 'tool' | 'success' | 'warning' | 'error' | 'inactive'

  interface Props {
    status?: Status
    pulse?: boolean
    size?: number
  }

  let { status = 'idle', pulse = false, size = 8 }: Props = $props()
</script>

<span
  class="status-dot {status}"
  class:pulse
  style:width="{size}px"
  style:height="{size}px"
  role="status"
  aria-label={status}
></span>

<style>
  .status-dot {
    display: inline-block;
    border-radius: 50%;
    flex-shrink: 0;
    background: var(--c-text-faint);
  }

  .status-dot.idle {
    background: var(--c-text-muted);
  }

  .status-dot.thinking {
    background: var(--c-accent);
  }

  .status-dot.tool {
    background: var(--c-generate);
  }

  .status-dot.success {
    background: var(--c-success);
  }

  .status-dot.warning {
    background: var(--c-warning);
  }

  .status-dot.error {
    background: var(--c-danger);
  }

  .status-dot.inactive {
    background: var(--c-text-faint);
  }

  .status-dot.pulse {
    animation: pulse 1.4s ease-in-out infinite;
  }

  @keyframes pulse {
    0%,
    100% {
      opacity: 1;
    }
    50% {
      opacity: 0.4;
    }
  }
</style>
