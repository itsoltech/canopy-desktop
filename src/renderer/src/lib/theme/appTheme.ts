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
  const bg = hexToRgb(theme.background as string)
  const fg = hexToRgb(theme.foreground as string)
  const light = isLightTheme(theme.background as string)
  const accent = hexToRgb(theme.blue as string)
  const red = hexToRgb(theme.red as string)
  const green = hexToRgb(theme.green as string)
  const yellow = hexToRgb(theme.yellow as string)
  const magenta = hexToRgb(theme.magenta as string)

  const elevated = light ? blend(bg, [0, 0, 0], 0.05) : blend(bg, [255, 255, 255], 0.05)

  const contrastBase: RGB = light ? [0, 0, 0] : [255, 255, 255]

  return {
    '--c-bg': rgb(bg),
    '--c-bg-elevated': rgb(elevated),
    '--c-bg-glass': rgba(bg, 0.75),
    '--c-bg-glass-heavy': rgba(bg, 0.85),
    '--c-bg-glass-light': rgba(bg, 0.6),
    '--c-bg-overlay': rgba(bg, 0.98),
    '--c-bg-input': rgba(contrastBase, 0.3),

    '--c-text': rgba(fg, 0.86),
    '--c-text-secondary': rgba(fg, 0.6),
    '--c-text-muted': rgba(fg, 0.4),
    '--c-text-faint': rgba(fg, 0.25),

    '--c-border': rgba(contrastBase, 0.12),
    '--c-border-subtle': rgba(contrastBase, 0.06),

    '--c-hover': rgba(contrastBase, 0.06),
    '--c-hover-strong': rgba(contrastBase, 0.1),
    '--c-active': rgba(contrastBase, 0.08),
    '--c-focus-ring': rgba(accent, 0.6),

    '--c-accent': rgb(accent),
    '--c-accent-bg': rgba(accent, 0.15),
    '--c-accent-bg-hover': rgba(accent, 0.25),
    '--c-accent-text': rgba(accent, 0.9),
    '--c-accent-muted': rgba(accent, 0.3),

    '--c-danger': rgb(red),
    '--c-danger-bg': rgba(red, 0.2),
    '--c-danger-text': rgba(red, 0.9),
    '--c-success': rgb(green),
    '--c-warning': rgb(yellow),
    '--c-warning-text': rgba(yellow, 0.9),
    '--c-generate': rgb(magenta),

    '--c-scrollbar': rgba(contrastBase, 0.15),
    '--c-scrollbar-hover': rgba(contrastBase, 0.3),
    '--c-scrollbar-active': rgba(contrastBase, 0.4),

    '--c-scrim': 'rgba(0, 0, 0, 0.5)',
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
