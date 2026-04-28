<script lang="ts">
  import { ClaudeAILogo, OpenAILogo, GeminiLogo, GitLogo } from '@selemondev/svgl-svelte'
  import { Terminal, Globe, FileText, Code } from 'lucide-svelte'

  let { icon, size = 14 }: { icon: string; size?: number } = $props()
</script>

{#if icon === 'ClaudeAI'}
  <ClaudeAILogo width={size} height={size} />
{:else if icon === 'OpenAI'}
  <OpenAILogo width={size} height={size} />
{:else if icon === 'Gemini'}
  <GeminiLogo width={size} height={size} />
{:else if icon === 'Git'}
  <GitLogo width={size} height={size} />
{:else if icon === 'code' || icon === 'OpenCode'}
  <Code {size} />
{:else if icon === 'terminal'}
  <Terminal {size} />
{:else if icon === 'Globe'}
  <Globe {size} />
{:else if icon === 'editor' || icon === 'FileText'}
  <FileText {size} />
{:else}
  <span
    class="inline-flex items-center justify-center rounded-sm bg-hover-strong text-text-secondary font-semibold flex-shrink-0"
    style:width="{size}px"
    style:height="{size}px"
    style:font-size="{size * 0.6}px"
  >
    {icon.charAt(0).toUpperCase()}
  </span>
{/if}
