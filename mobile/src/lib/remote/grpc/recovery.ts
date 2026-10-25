// View-owned subscriptions retry reads only. Abort also cancels the backoff timer.
export async function recoveryDelay(attempt: number, signal: AbortSignal): Promise<void> {
  if (signal.aborted) return
  await new Promise<void>(resolve => {
    const finish = () => { clearTimeout(timer); signal.removeEventListener('abort', finish); resolve() }
    const timer = setTimeout(finish, Math.min(2000, 100 * 2 ** Math.min(attempt, 5)))
    signal.addEventListener('abort', finish, { once: true })
  })
}
