import type { ITheme } from '@xterm/xterm'

type RGB = [number, number, number]

function hexToRgb(hex: string): RGB {
  const h = hex.replace('#', '')
  if (h.length === 3) {
    return [parseInt(h[0] + h[0], 16), parseInt(h[1] + h[1], 16), parseInt(h[2] + h[2], 16)]
  }
  return [parseInt(h.slice(0, 2), 16), parseInt(h.slice(2, 4), 16), parseInt(h.slice(4, 6), 16)]
}

function isLightTheme(bg: string): boolean {
  const [r, g, b] = hexToRgb(bg)
  return (0.299 * r + 0.587 * g + 0.114 * b) / 255 > 0.5
}

function rgba([r, g, b]: RGB, a: number): string {
  return `rgba(${r}, ${g}, ${b}, ${a})`
}

function rgb([r, g, b]: RGB): string {
  return `rgb(${r}, ${g}, ${b})`
}

function blend(base: RGB, target: RGB, amount: number): RGB {
  return [
    Math.round(base[0] + (target[0] - base[0]) * amount),
    Math.round(base[1] + (target[1] - base[1]) * amount),
    Math.round(base[2] + (target[2] - base[2]) * amount),
  ]
}

export function deriveAppTheme(theme: ITheme): Record<string, string> {
  // All built-in themes populate these fields; getTheme() guarantees a built-in.
  const bg = hexToRgb(theme.background as string)
  const fg = hexToRgb(theme.foreground as string)
  const light = isLightTheme(theme.background as string)
  const accent = hexToRgb(theme.blue as string)
  const red = hexToRgb(theme.red as string)
  const green = hexToRgb(theme.green as string)
  const yellow = hexToRgb(theme.yellow as string)
  const magenta = hexToRgb(theme.magenta as string)
  const cyan = hexToRgb((theme.cyan as string) ?? (theme.brightBlue as string) ?? '#00bcd4')

  const elevated = light ? blend(bg, [0, 0, 0], 0.05) : blend(bg, [255, 255, 255], 0.05)

  const contrastBase: RGB = light ? [0, 0, 0] : [255, 255, 255]

  return {
    '--color-bg': rgb(bg),
    '--color-bg-elevated': rgb(elevated),
    '--color-bg-glass': rgba(bg, 0.75),
    '--color-bg-glass-heavy': rgba(bg, 0.85),
    '--color-bg-glass-light': rgba(bg, 0.6),
    '--color-bg-overlay': rgba(bg, 0.98),
    '--color-bg-input': rgba([0, 0, 0], light ? 0.06 : 0.3),

    '--color-text': rgba(fg, 0.86),
    '--color-text-secondary': rgba(fg, 0.6),
    '--color-text-muted': rgba(fg, 0.4),
    '--color-text-faint': rgba(fg, 0.25),

    '--color-border': rgba(contrastBase, 0.12),
    '--color-border-subtle': rgba(contrastBase, 0.06),

    '--color-hover': rgba(contrastBase, 0.06),
    '--color-hover-strong': rgba(contrastBase, 0.1),
    '--color-active': rgba(contrastBase, 0.08),
    '--color-focus-ring': rgba(accent, 0.6),

    '--color-accent': rgb(accent),
    '--color-accent-bg': rgba(accent, 0.15),
    '--color-accent-bg-hover': rgba(accent, 0.25),
    '--color-accent-text': rgba(accent, 0.9),
    '--color-accent-muted': rgba(accent, 0.3),

    '--color-danger': rgb(red),
    '--color-danger-bg': rgba(red, 0.2),
    '--color-danger-text': rgba(red, 0.9),
    '--color-success': rgb(green),
    '--color-warning': rgb(yellow),
    '--color-warning-text': rgba(yellow, 0.9),
    '--color-generate': rgb(magenta),

    '--color-scrollbar': rgba(contrastBase, 0.15),
    '--color-scrollbar-hover': rgba(contrastBase, 0.3),
    '--color-scrollbar-active': rgba(contrastBase, 0.4),

    '--color-scrim': 'rgba(0, 0, 0, 0.5)',

    // Syntax highlighting (derived from xterm ANSI palette so it tracks any theme)
    '--color-syntax-keyword': rgb(magenta),
    '--color-syntax-string': rgb(green),
    '--color-syntax-comment': rgba(fg, 0.45),
    '--color-syntax-number': rgb(yellow),
    '--color-syntax-operator': rgba(fg, 0.75),
    '--color-syntax-function': rgb(yellow),
    '--color-syntax-variable': rgb(accent),
    '--color-syntax-property': rgb(accent),
    '--color-syntax-type': rgb(cyan),
    '--color-syntax-tag': rgb(accent),
    '--color-syntax-heading': rgb(accent),
    '--color-syntax-error': rgb(red),
  }
}

export function applyAppTheme(theme: ITheme): void {
  const vars = deriveAppTheme(theme)
  const root = document.documentElement
  for (const [key, value] of Object.entries(vars)) {
    root.style.setProperty(key, value)
  }
  const light = isLightTheme(theme.background as string)
  root.style.setProperty('color-scheme', light ? 'light' : 'dark')
}
