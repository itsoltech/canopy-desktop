import type { WebContents } from 'electron'
import type { IPty } from 'node-pty'

const MAX_HISTORY_BYTES = 1_048_576

interface HistoryChunk {
  start: number
  end: number
  bytes: number
  data: string
}

interface TerminalStream {
  sessionId: string
  ownerWebContentsId: number
  history: HistoryChunk[]
  historyBytes: number
  totalChars: number
  subscribers: Set<string>
  cleanup: () => void
}

interface TerminalStreamSubscriber {
  subscriptionId: string
  sessionId: string
  webContents: WebContents
  onDestroyed: () => void
}

export interface TerminalStreamSubscribeInput {
  webContents: WebContents
  sessionId: string
  subscriptionId: string
  offset: number
}

export interface TerminalStreamDataEvent {
  subscriptionId: string
  sessionId: string
  offset: number
  data: string
}

export interface TerminalStreamClosedEvent {
  subscriptionId: string
  sessionId: string
}

export interface TerminalStreamDiagnostics {
  terminalStreamCount: number
  terminalStreamSubscriberCount: number
  totalHistoryBytes: number
  streams: Array<{
    sessionId: string
    ownerWebContentsId: number
    subscriberCount: number
    historyBytes: number
    oldestOffset: number
    currentOffset: number
  }>
}

export interface TerminalStreamServiceOptions {
  ownsSession?: (webContentsId: number, sessionId: string) => boolean
}

export class TerminalStreamService {
  private streams = new Map<string, TerminalStream>()
  private subscribers = new Map<string, TerminalStreamSubscriber>()
  private ownsSession: (webContentsId: number, sessionId: string) => boolean

  constructor(options?: TerminalStreamServiceOptions) {
    this.ownsSession = options?.ownsSession ?? (() => false)
  }

  register(sessionId: string, ptyProcess: IPty, ownerWebContentsId: number): void {
    this.destroy(sessionId)

    const stream: TerminalStream = {
      sessionId,
      ownerWebContentsId,
      history: [],
      historyBytes: 0,
      totalChars: 0,
      subscribers: new Set(),
      cleanup: () => {},
    }

    const onData = ptyProcess.onData((data) => {
      this.appendHistory(stream, data)
      this.broadcastData(stream, stream.totalChars - data.length, data)
    })
    const onExit = ptyProcess.onExit(() => this.destroy(sessionId))

    stream.cleanup = () => {
      onData.dispose()
      onExit.dispose()
    }

    this.streams.set(sessionId, stream)
  }

  destroy(sessionId: string): void {
    const stream = this.streams.get(sessionId)
    if (!stream) return

    for (const subscriptionId of [...stream.subscribers]) {
      this.closeSubscriber(subscriptionId)
    }
    stream.cleanup()
    this.streams.delete(sessionId)
  }

  disposeAll(): void {
    for (const sessionId of [...this.streams.keys()]) {
      this.destroy(sessionId)
    }
  }

  hasStream(webContentsId: number, sessionId: string): boolean {
    const stream = this.streams.get(sessionId)
    if (!stream) return false
    return stream.ownerWebContentsId === webContentsId || this.ownsSession(webContentsId, sessionId)
  }

  subscribe(input: TerminalStreamSubscribeInput): void {
    const { webContents, sessionId, subscriptionId } = input
    const stream = this.streams.get(sessionId)
    if (!stream) {
      throw new Error('PTY stream does not exist')
    }
    if (
      stream.ownerWebContentsId !== webContents.id &&
      !this.ownsSession(webContents.id, sessionId)
    ) {
      throw new Error('PTY session is not owned by this window')
    }
    if (this.subscribers.has(subscriptionId)) {
      throw new Error('PTY stream subscription already exists')
    }

    const onDestroyed = (): void => {
      this.unsubscribe(subscriptionId)
    }
    const subscriber: TerminalStreamSubscriber = {
      subscriptionId,
      sessionId,
      webContents,
      onDestroyed,
    }
    this.subscribers.set(subscriptionId, subscriber)
    stream.subscribers.add(subscriptionId)

    webContents.once('destroyed', onDestroyed)

    const offset = this.normalizeOffset(input.offset)
    this.sendReplay(stream, subscriber, offset)
  }

  unsubscribe(subscriptionId: string, webContents?: WebContents): boolean {
    const subscriber = this.subscribers.get(subscriptionId)
    if (!subscriber) return false
    if (webContents && subscriber.webContents.id !== webContents.id) {
      throw new Error('PTY stream subscription is not owned by this window')
    }

    const stream = this.streams.get(subscriber.sessionId)
    stream?.subscribers.delete(subscriptionId)
    subscriber.webContents.removeListener('destroyed', subscriber.onDestroyed)
    this.subscribers.delete(subscriptionId)
    return true
  }

  pauseAll(reason?: string): number {
    void reason
    return this.disconnectAllSubscribers()
  }

  disconnectAllSubscribers(): number {
    let disconnected = 0
    for (const subscriptionId of [...this.subscribers.keys()]) {
      if (this.unsubscribe(subscriptionId)) disconnected++
    }
    return disconnected
  }

  getDiagnostics(): TerminalStreamDiagnostics {
    const streams = [...this.streams.values()].map((stream) => ({
      sessionId: stream.sessionId,
      ownerWebContentsId: stream.ownerWebContentsId,
      subscriberCount: stream.subscribers.size,
      historyBytes: stream.historyBytes,
      oldestOffset: stream.history[0]?.start ?? stream.totalChars,
      currentOffset: stream.totalChars,
    }))

    return {
      terminalStreamCount: this.streams.size,
      terminalStreamSubscriberCount: this.subscribers.size,
      totalHistoryBytes: streams.reduce((total, stream) => total + stream.historyBytes, 0),
      streams,
    }
  }

  private appendHistory(stream: TerminalStream, data: string): void {
    const start = stream.totalChars
    stream.totalChars += data.length
    const chunk: HistoryChunk = {
      start,
      end: stream.totalChars,
      bytes: Buffer.byteLength(data, 'utf-8'),
      data,
    }
    stream.history.push(chunk)
    stream.historyBytes += chunk.bytes

    while (stream.historyBytes > MAX_HISTORY_BYTES && stream.history.length > 0) {
      const removed = stream.history.shift()
      if (!removed) break
      stream.historyBytes -= removed.bytes
    }
  }

  private broadcastData(stream: TerminalStream, offset: number, data: string): void {
    for (const subscriptionId of [...stream.subscribers]) {
      this.sendData(subscriptionId, offset, data)
    }
  }

  private sendReplay(
    stream: TerminalStream,
    subscriber: TerminalStreamSubscriber,
    requestedOffset: number,
  ): void {
    for (const chunk of stream.history) {
      if (chunk.end <= requestedOffset) continue
      if (requestedOffset > chunk.start) {
        const data = chunk.data.slice(requestedOffset - chunk.start)
        this.sendData(subscriber.subscriptionId, requestedOffset, data)
      } else {
        this.sendData(subscriber.subscriptionId, chunk.start, chunk.data)
      }
    }
  }

  private sendData(subscriptionId: string, offset: number, data: string): void {
    const subscriber = this.subscribers.get(subscriptionId)
    if (!subscriber) return
    if (subscriber.webContents.isDestroyed()) {
      this.unsubscribe(subscriptionId)
      return
    }

    const payload: TerminalStreamDataEvent = {
      subscriptionId,
      sessionId: subscriber.sessionId,
      offset,
      data,
    }
    subscriber.webContents.send('pty-stream:data', payload)
  }

  private closeSubscriber(subscriptionId: string): boolean {
    const subscriber = this.subscribers.get(subscriptionId)
    if (!subscriber) return false

    this.unsubscribe(subscriptionId)
    if (!subscriber.webContents.isDestroyed()) {
      const payload: TerminalStreamClosedEvent = {
        subscriptionId,
        sessionId: subscriber.sessionId,
      }
      subscriber.webContents.send('pty-stream:closed', payload)
    }
    return true
  }

  private normalizeOffset(offset: number): number {
    return Number.isFinite(offset) && offset >= 0 ? Math.floor(offset) : 0
  }
}
