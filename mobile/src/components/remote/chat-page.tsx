import React, { useEffect, useRef, useState } from 'react'
import { WebView } from 'react-native-webview'
import { Action } from './common'
import { readChatPage } from '@/lib/remote/grpc/artifact-preview'
import type { HostSession } from '@/lib/remote/grpc/hosts'
import type { ThreadRef, ItemKey } from '@/lib/remote/grpc/schema'
export function IsolatedHtml({ html }: { html: string }) {
  const escaped = html.replace(/&/g, '&amp;').replace(/"/g, '&quot;').replace(/</g, '&lt;')
  // No native message bridge, remote credentials, same-origin privilege, navigation or network.
  const page = `<html><head><meta name="viewport" content="width=device-width"><meta http-equiv="Content-Security-Policy" content="default-src 'none'; script-src 'unsafe-inline'; style-src 'unsafe-inline'; img-src data: blob:; media-src data: blob:; font-src data:; frame-src about:;"></head><body style="margin:0"><iframe sandbox="allow-scripts" style="border:0;width:100%;height:100vh" srcdoc="${escaped}"></iframe></body></html>`
  return <WebView style={{ height: 400 }} source={{ html: page }} javaScriptEnabled originWhitelist={['about:blank', 'about:srcdoc']} allowFileAccess={false} allowFileAccessFromFileURLs={false} allowUniversalAccessFromFileURLs={false} sharedCookiesEnabled={false} thirdPartyCookiesEnabled={false} javaScriptCanOpenWindowsAutomatically={false} onShouldStartLoadWithRequest={request => request.url === 'about:blank' || request.url === 'about:srcdoc'} />
}
export function ChatPage({ host, chat, item }: { host: HostSession; chat: ThreadRef; item: ItemKey }) {
  const [html, setHtml] = useState<string>(), id = useRef<string | undefined>(undefined), abort = useRef(new AbortController())
  useEffect(() => () => { abort.current.abort(); if (id.current) void host.transport.native.removeAttachment(id.current).catch(() => {}) }, [host])
  return <><Action title={html === undefined ? 'Open interactive page' : 'Reload page'} run={async () => {
    const result = await readChatPage(host, chat, item, abort.current.signal)
    if (abort.current.signal.aborted) { await host.transport.native.removeAttachment(result.id); return }
    const previous = id.current; id.current = result.id; setHtml(result.html)
    if (previous) await host.transport.native.removeAttachment(previous)
  }} />{html !== undefined && <IsolatedHtml html={html} />}</>
}
