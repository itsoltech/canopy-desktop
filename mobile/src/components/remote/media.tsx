import { IsolatedHtml } from './chat-page'
import React from 'react'
import { Image } from 'expo-image'
import { WebView } from 'react-native-webview'
import { ThemedText } from '@/components/themed-text'
// Embedded media is a renderer only; all host I/O is native gRPC outside WebView.
export function MediaPreview({ uri, mediaType, html: pageHtml }: { uri: string; mediaType: string; html?: string }) {
  if (mediaType === 'text/html') return pageHtml !== undefined ? <IsolatedHtml html={pageHtml} /> : <ThemedText>Page bytes are unavailable.</ThemedText>
  if (mediaType.startsWith('image/')) return <Image source={{ uri }} style={{ height: 280, width: '100%' }} contentFit="contain" accessibilityLabel="Image preview" />
  const escaped = uri.replace(/&/g, '&amp;').replace(/"/g, '&quot;').replace(/</g, '&lt;')
  const html = mediaType.startsWith('video/') ? `<html><meta name="viewport" content="width=device-width"><body style="margin:0;background:black"><video controls playsinline style="width:100%;height:100%" src="${escaped}"></video></body></html>`
    : mediaType.startsWith('audio/') ? `<html><meta name="viewport" content="width=device-width"><audio controls src="${escaped}"></audio></html>`
    : mediaType.startsWith('font/') || ['application/font-woff', 'application/vnd.ms-opentype'].includes(mediaType) ? `<html><meta name="viewport" content="width=device-width"><style>@font-face{font-family:preview;src:url("${escaped}")}body{font-family:preview;font-size:28px;padding:16px}</style><body contenteditable="true">The quick brown fox jumps over the lazy dog.<br>ABCDEFGHIJKLMNOPQRSTUVWXYZ<br>abcdefghijklmnopqrstuvwxyz<br>0123456789</body></html>` : undefined
  if (html || mediaType === 'application/pdf') return <WebView style={{ height: 360 }} source={html ? { html, baseUrl: uri } : { uri }} originWhitelist={['file://*', 'about:blank']} allowingReadAccessToURL={uri} allowFileAccess allowFileAccessFromFileURLs={false} allowUniversalAccessFromFileURLs={false} javaScriptEnabled={false} onShouldStartLoadWithRequest={request => request.url === 'about:blank' || request.url.split('#')[0] === uri} />
  return <ThemedText type="small">Preview unavailable for {mediaType}. The source remains on the host.</ThemedText>
}
