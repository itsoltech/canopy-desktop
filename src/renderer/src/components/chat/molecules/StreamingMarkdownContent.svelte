<script lang="ts">
  import { onDestroy } from 'svelte'
  import MarkdownContent from './MarkdownContent.svelte'

  interface Props {
    content: string
    active?: boolean
    tokenDelayMs?: number
    onreveal?: () => void
  }

  let { content, active = false, tokenDelayMs = 35, onreveal }: Props = $props()

  let renderedContent = $state(active ? '' : content)
  let animatingToken = $state(false)
  let queuedContent = ''
  let timer: ReturnType<typeof setTimeout> | null = null

  function clearTimer(): void {
    if (timer === null) return
    clearTimeout(timer)
    timer = null
  }

  function scheduleNextToken(): void {
    if (timer !== null || queuedContent.length === 0) return

    timer = setTimeout(() => {
      timer = null
      const nextToken = nextQueuedToken(queuedContent)
      queuedContent = queuedContent.slice(nextToken.length)
      renderedContent = renderedContent + nextToken
      animatingToken = true
      onreveal?.()
      scheduleNextToken()
    }, tokenDelayMs)
  }

  function nextQueuedToken(value: string): string {
    return value.match(/^\s*\S+\s*/)?.[0] ?? value[0]
  }

  $effect(() => {
    if (!active) {
      if (content.startsWith(renderedContent) && content !== renderedContent) {
        queuedContent = content.slice(renderedContent.length)
        scheduleNextToken()
        return
      }
      clearTimer()
      queuedContent = ''
      renderedContent = content
      animatingToken = false
      return
    }

    if (content === renderedContent + queuedContent) {
      scheduleNextToken()
      return
    }

    if (content.startsWith(renderedContent)) {
      queuedContent = content.slice(renderedContent.length)
      scheduleNextToken()
      return
    }

    clearTimer()
    queuedContent = ''
    renderedContent = content
    animatingToken = false
  })

  onDestroy(clearTimer)
</script>

<MarkdownContent content={renderedContent} animateLastToken={animatingToken && active} />
