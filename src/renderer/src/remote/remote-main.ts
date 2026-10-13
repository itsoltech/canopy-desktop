import { mount } from 'svelte'

import './assets/main.css'

import RemoteApp from './RemoteApp.svelte'

const MAX_BANNER_CHARS = 2000

let bannerEl: HTMLDivElement | null = null
let bannerText: HTMLSpanElement | null = null

// Reuse one banner so a burst of errors does not stack up and cover the page.
function ensureBanner(): HTMLSpanElement {
  if (bannerEl && bannerText && bannerEl.isConnected) return bannerText
  bannerEl = document.createElement('div')
  bannerEl.setAttribute(
    'style',
    'position:fixed;top:0;left:0;right:0;z-index:9999;' +
      'background:#2a1515;color:#ffcccc;font:12px/1.4 ui-monospace,monospace;' +
      'padding:12px 16px;white-space:pre-wrap;word-break:break-all;' +
      'border-bottom:1px solid #550000;max-height:50vh;overflow:auto',
  )
  bannerText = document.createElement('span')
  const dismiss = document.createElement('button')
  dismiss.type = 'button'
  dismiss.textContent = 'Dismiss'
  dismiss.setAttribute('style', 'float:right;margin-left:12px;font:inherit;cursor:pointer')
  dismiss.addEventListener('click', () => bannerEl?.remove())
  bannerEl.append(dismiss, bannerText)
  document.body.appendChild(bannerEl)
  return bannerText
}

// Surface any unhandled errors at the top of the page so users who hit
// a bug on their phone don't have to connect Safari Web Inspector to
// figure out what's wrong. Without this the whole SPA can silently
// crash and the only visible symptom is a frozen "Connecting…" screen.
function showFatalError(label: string, err: unknown): void {
  const message = err instanceof Error ? `${err.message}\n${err.stack ?? ''}` : String(err)
  console.error(`[remote:${label}]`, err)
  ensureBanner().textContent = `[${label}] ${message}`.slice(0, MAX_BANNER_CHARS)
}

window.addEventListener('error', (e) => {
  showFatalError('uncaught', e.error ?? e.message)
})
window.addEventListener('unhandledrejection', (e) => {
  showFatalError('unhandled-promise', e.reason)
})

const app = mount(RemoteApp, {
  target: document.getElementById('app')!,
})

export default app
