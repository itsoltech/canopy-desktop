export function startSettledPoll(task: () => Promise<void>, intervalMs: number): () => void {
  let cancelled = false
  let timer: ReturnType<typeof setTimeout> | undefined

  const poll = async (): Promise<void> => {
    await task()
    if (!cancelled) timer = setTimeout(() => void poll(), intervalMs)
  }

  void poll()
  return () => {
    cancelled = true
    if (timer) clearTimeout(timer)
  }
}
