<script lang="ts">
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
    switch (ext) {
      case 'ts':
      case 'tsx':
      case 'cts':
      case 'mts':
        return { Icon: FileCode, tone: 'text-accent-text' }
      case 'js':
      case 'jsx':
      case 'cjs':
      case 'mjs':
        return { Icon: FileCode, tone: 'text-warning-text' }
      case 'svelte':
      case 'vue':
        return { Icon: FileCode, tone: 'text-danger-text' }
      case 'json':
      case 'jsonc':
        return { Icon: FileJson, tone: 'text-warning-text' }
      case 'yaml':
      case 'yml':
      case 'toml':
        return { Icon: FileCog, tone: 'text-text-muted' }
      case 'css':
      case 'scss':
      case 'sass':
      case 'less':
        return { Icon: FileType, tone: 'text-accent-text' }
      case 'md':
      case 'mdx':
        return { Icon: FileText, tone: 'text-success-text' }
      case 'txt':
      case 'log':
        return { Icon: FileText, tone: 'text-text-muted' }
      case 'rs':
      case 'go':
      case 'py':
      case 'rb':
      case 'sh':
      case 'fish':
      case 'zsh':
      case 'bash':
        return { Icon: FileCode, tone: 'text-text-muted' }
      default:
        return { Icon: File, tone: 'text-text-muted' }
    }
  }

  let resolved = $derived(resolve(name))
  let Icon = $derived(resolved.Icon)
</script>

<Icon {size} class="flex-shrink-0 {resolved.tone}" />
