<script lang="ts">
  import { match, P } from 'ts-pattern'
  import {
    File,
    FileCode,
    FileJson,
    FileText,
    FileType,
    FileCog,
    BookOpen,
    Package,
    Diamond,
  } from '@lucide/svelte'

  let { name, size = 13 }: { name: string; size?: number } = $props()

  type IconDef = {
    Icon: typeof File
    tone: string
  }

  function resolve(filename: string): IconDef {
    const lower = filename.toLowerCase()

    if (lower === 'package.json' || lower === 'package-lock.json' || lower === 'bun.lockb') {
      return { Icon: Package, tone: 'text-warning-text' }
    }
    if (lower.startsWith('readme')) {
      return { Icon: BookOpen, tone: 'text-success-text' }
    }
    if (lower === '.gitignore' || lower === '.gitattributes' || lower === '.gitmodules') {
      return { Icon: Diamond, tone: 'text-danger-text' }
    }
    if (lower === '.editorconfig' || lower.endsWith('.config.js') || lower.endsWith('.config.ts')) {
      return { Icon: FileCog, tone: 'text-text-muted' }
    }

    const ext = lower.includes('.') ? lower.slice(lower.lastIndexOf('.') + 1) : ''
    return match(ext)
      .with(P.union('ts', 'tsx', 'cts', 'mts'), () => ({
        Icon: FileCode,
        tone: 'text-accent-text',
      }))
      .with(P.union('js', 'jsx', 'cjs', 'mjs'), () => ({
        Icon: FileCode,
        tone: 'text-warning-text',
      }))
      .with(P.union('svelte', 'vue'), () => ({ Icon: FileCode, tone: 'text-danger-text' }))
      .with(P.union('json', 'jsonc'), () => ({ Icon: FileJson, tone: 'text-warning-text' }))
      .with(P.union('yaml', 'yml', 'toml'), () => ({ Icon: FileCog, tone: 'text-text-muted' }))
      .with(P.union('css', 'scss', 'sass', 'less'), () => ({
        Icon: FileType,
        tone: 'text-accent-text',
      }))
      .with(P.union('md', 'mdx'), () => ({ Icon: FileText, tone: 'text-success-text' }))
      .with(P.union('txt', 'log'), () => ({ Icon: FileText, tone: 'text-text-muted' }))
      .with(P.union('rs', 'go', 'py', 'rb', 'sh', 'fish', 'zsh', 'bash'), () => ({
        Icon: FileCode,
        tone: 'text-text-muted',
      }))
      .otherwise(() => ({ Icon: File, tone: 'text-text-muted' }))
  }

  let resolved = $derived(resolve(name))
  let Icon = $derived(resolved.Icon)
</script>

<Icon {size} class="flex-shrink-0 {resolved.tone}" />
