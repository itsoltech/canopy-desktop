<script lang="ts">
  import { ClaudeAILogo, OpenAILogo, GeminiLogo, GitLogo } from '@selemondev/svgl-svelte'
  import { Terminal, Globe, FileText } from 'lucide-svelte'

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
{:else if icon === 'terminal'}
  <Terminal {size} />
{:else if icon === 'Globe'}
  <Globe {size} />
{:else if icon === 'editor' || icon === 'FileText'}
  <FileText {size} />
{:else}
  <span
    class="fallback"
    style:width="{size}px"
    style:height="{size}px"
    style:font-size="{size * 0.6}px"
  >
    {icon.charAt(0).toUpperCase()}
  </span>
{/if}

<style>
  .fallback {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    border-radius: 3px;
    background: rgba(255, 255, 255, 0.1);
    color: rgba(255, 255, 255, 0.5);
    font-weight: 600;
    flex-shrink: 0;
  }
</style>
