/** Minimal type for Electron <webview> element methods used in the renderer */
export interface WebviewElement extends HTMLElement {
  loadURL(url: string): Promise<void>
  getURL(): string
  getTitle(): string
  canGoBack(): boolean
  canGoForward(): boolean
  goBack(): void
  goForward(): void
  reload(): void
  isDevToolsOpened(): boolean
  getWebContentsId(): number
  openDevTools(): void
  closeDevTools(): void
  isDevToolsFocused(): boolean
  executeJavaScript(code: string): Promise<unknown>
  capturePage(rect?: { x: number; y: number; width: number; height: number }): Promise<{
    toPNG(): Buffer
    toDataURL(): string
  }>
}

import { prefs, setPref } from '../stores/preferences.svelte'

export interface ViewportPreset {
  width: number
  height: number
  scaleFactor: number
  mobile: boolean
}

export const DEFAULT_VIEWPORTS: Record<string, ViewportPreset> = {
  'iPhone SE': { width: 375, height: 667, scaleFactor: 2, mobile: true },
  'iPhone 14 Pro': { width: 393, height: 852, scaleFactor: 3, mobile: true },
  'iPhone 14 Pro Max': { width: 430, height: 932, scaleFactor: 3, mobile: true },
  'iPad Mini': { width: 768, height: 1024, scaleFactor: 2, mobile: true },
  'iPad Pro 11"': { width: 834, height: 1194, scaleFactor: 2, mobile: true },
  'Samsung Galaxy S24': { width: 360, height: 780, scaleFactor: 3, mobile: true },
  'Pixel 8': { width: 412, height: 915, scaleFactor: 2.625, mobile: true },
}

export function getCustomViewports(): Record<string, ViewportPreset> {
  const raw = prefs['viewports.custom']
  if (!raw) return {}
  try {
    return JSON.parse(raw) as Record<string, ViewportPreset>
  } catch {
    return {}
  }
}

export function getAllViewports(): Record<string, ViewportPreset> {
  return { ...DEFAULT_VIEWPORTS, ...getCustomViewports() }
}

export function saveCustomViewports(viewports: Record<string, ViewportPreset>): void {
  setPref('viewports.custom', JSON.stringify(viewports))
}

export interface BrowserFavorite {
  url: string
  name: string
  favicon: string | null
}

export function getFavorites(): BrowserFavorite[] {
  const raw = prefs['browser.favorites']
  if (!raw) return []
  try {
    return JSON.parse(raw) as BrowserFavorite[]
  } catch {
    return []
  }
}

export function addFavorite(fav: BrowserFavorite): void {
  const list = getFavorites().filter((f) => f.url !== fav.url)
  list.push(fav)
  setPref('browser.favorites', JSON.stringify(list))
}

export function removeFavorite(url: string): void {
  const list = getFavorites().filter((f) => f.url !== url)
  setPref('browser.favorites', JSON.stringify(list))
}

export function updateFavorite(oldUrl: string, updated: BrowserFavorite): void {
  const list = getFavorites().map((f) => (f.url === oldUrl ? updated : f))
  setPref('browser.favorites', JSON.stringify(list))
}

export function reorderFavorites(fromIndex: number, toIndex: number): void {
  const list = getFavorites()
  const [item] = list.splice(fromIndex, 1)
  list.splice(toIndex, 0, item)
  setPref('browser.favorites', JSON.stringify(list))
}

export function isFavorite(url: string): boolean {
  return getFavorites().some((f) => f.url === url)
}

export interface BrowserSessionState {
  url: string
  title: string
  favicon: string | null
  canGoBack: boolean
  canGoForward: boolean
  isLoading: boolean
  isDevToolsOpen: boolean
  devToolsMode: 'bottom' | 'left'
  error: { code: number; description: string; url: string } | null
}

export const browserSessions: Record<string, BrowserSessionState> = $state({})

export function initBrowserSession(browserId: string): void {
  // Preserve existing state across component remounts (e.g. drag-to-split)
  if (browserSessions[browserId]) return
  browserSessions[browserId] = {
    url: '',
    title: '',
    favicon: null,
    canGoBack: false,
    canGoForward: false,
    isLoading: false,
    isDevToolsOpen: false,
    devToolsMode: 'bottom',
    error: null,
  }
}

export function handleBrowserUrlChanged(browserId: string, url: string): void {
  const session = browserSessions[browserId]
  if (session) {
    session.url = url
    session.error = null
  }
}

export function handleBrowserTitleChanged(browserId: string, title: string): void {
  const session = browserSessions[browserId]
  if (session) {
    session.title = title
  }
}

export function handleBrowserFaviconChanged(browserId: string, favicon: string | null): void {
  const session = browserSessions[browserId]
  if (session) {
    session.favicon = favicon
  }
}

export function handleBrowserLoadingChanged(browserId: string, isLoading: boolean): void {
  const session = browserSessions[browserId]
  if (session) {
    session.isLoading = isLoading
    if (isLoading) {
      session.error = null
    }
  }
}

export function handleBrowserLoadFailed(
  browserId: string,
  errorCode: number,
  errorDescription: string,
  validatedURL: string,
): void {
  const session = browserSessions[browserId]
  if (session) {
    session.error = { code: errorCode, description: errorDescription, url: validatedURL }
  }
}

export function handleBrowserStateChanged(
  browserId: string,
  state: {
    canGoBack: boolean
    canGoForward: boolean
    isDevToolsOpen: boolean
    devToolsMode: 'bottom' | 'left'
  },
): void {
  const session = browserSessions[browserId]
  if (session) {
    session.canGoBack = state.canGoBack
    session.canGoForward = state.canGoForward
    session.isDevToolsOpen = state.isDevToolsOpen
    session.devToolsMode = state.devToolsMode
  }
}
