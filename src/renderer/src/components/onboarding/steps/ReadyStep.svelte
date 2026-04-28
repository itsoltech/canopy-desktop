<script lang="ts">
  import { Check } from '@lucide/svelte'

  interface Props {
    onOpenFolder: () => void
  }

  let { onOpenFolder }: Props = $props()

  const isMac = navigator.userAgent.includes('Mac')
</script>

<div class="flex flex-col items-center text-center gap-3 py-6">
  <div class="w-20 h-20 rounded-[20px] bg-accent-bg flex items-center justify-center mb-2">
    <Check size={48} strokeWidth={1.5} color="var(--color-success)" />
  </div>
  <h2 class="m-0 text-[22px] font-semibold text-text">You're all set</h2>
  <p
    class="m-0 text-lg text-text-secondary leading-normal max-w-[360px] [&>kbd]:inline-block [&>kbd]:px-[5px] [&>kbd]:py-px [&>kbd]:text-sm [&>kbd]:font-inherit [&>kbd]:bg-active [&>kbd]:border [&>kbd]:border-text-faint [&>kbd]:rounded-md [&>kbd]:text-text-secondary"
  >
    Open a project folder to get started. You can always change your settings with
    <kbd>{isMac ? 'Cmd' : 'Ctrl'}+,</kbd>
  </p>

  <button
    class="mt-2 px-7 py-2.5 border-0 rounded-xl bg-accent-bg text-accent-text text-lg font-medium font-inherit cursor-pointer transition-colors duration-base hover:bg-accent-bg-hover focus-visible:outline-2 focus-visible:outline-focus-ring focus-visible:outline-offset-2"
    onclick={onOpenFolder}>Open folder</button
  >
</div>
