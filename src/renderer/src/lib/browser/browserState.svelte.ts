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
