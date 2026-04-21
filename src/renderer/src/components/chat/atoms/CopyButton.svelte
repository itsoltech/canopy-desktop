<script lang="ts">
  import { Copy, Check } from '@lucide/svelte'
  import IconButton from './IconButton.svelte'

  interface Props {
    text: string
    size?: number
    variant?: 'ghost' | 'primary' | 'danger'
  }

  let { text, size = 14, variant = 'ghost' }: Props = $props()

  let copied = $state(false)
  let timer: ReturnType<typeof setTimeout> | null = null

  async function copy(): Promise<void> {
    try {
      await navigator.clipboard.writeText(text)
      copied = true
      if (timer) clearTimeout(timer)
      timer = setTimeout(() => {
        copied = false
      }, 1500)
    } catch {
      // Clipboard unavailable — ignore silently.
    }
  }

  $effect(() => {
    return () => {
      if (timer) clearTimeout(timer)
    }
  })
</script>

<IconButton
  onclick={copy}
  tooltip={copied ? 'Copied' : 'Copy'}
  label={copied ? 'Copied' : 'Copy'}
  {variant}
>
  {#if copied}
    <Check {size} strokeWidth={2.5} />
  {:else}
    <Copy {size} />
  {/if}
</IconButton>
