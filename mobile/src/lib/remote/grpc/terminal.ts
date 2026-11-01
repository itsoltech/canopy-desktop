import { terminalKey, type KeyModifiers } from './terminal-keys'
import { recoveryDelay } from './recovery'
import type { HostSession } from './hosts'
import { TerminalInputMode } from './schema'
import type { TerminalHistoryStart, InputLease, TerminalRef, TerminalFrame, TerminalSessionRequest, TerminalServerEvent, Cursor } from './schema'
import type { RpcStream } from './transport'
import { checkReceipt, command, uniqueId } from './commands'
import { TerminalFrameAssembler } from './fragments'
export class TerminalViewOwner {
  readonly terminal: TerminalRef
  private stream?: RpcStream<TerminalSessionRequest, TerminalServerEvent>
  private abort?: AbortController
  private historyAbort?: AbortController
  private unsubscribe?: () => void
  private timer?: ReturnType<typeof setTimeout>
  private listeners = new Set<() => void>()
  private lease?: InputLease
  private leaseUntil = 0
  private frame?: TerminalFrame
  private history?: { frame: TerminalFrame; start: TerminalHistoryStart }
  private cursor?: Cursor
  private received?: Cursor
  private runId = ''
  private inputId = 0n
  private pendingInput?: string
  private ready = false
  private generation = 0
  private error?: string
  constructor(readonly host: HostSession, terminalId: string) { this.terminal = { host_id: host.snapshot().hostId, terminal_id: terminalId } }
  subscribe(callback: () => void): () => void { this.listeners.add(callback); return () => { this.listeners.delete(callback) } }
  snapshot() { return { frame: this.frame, history: this.history, lease: this.lease, error: this.error, pendingInput: this.pendingInput, runId: this.runId } }
  private publish() { for (const listener of this.listeners) listener() }
  start() {
    this.unsubscribe?.()
    const changed = () => { const ready = this.host.snapshot().phase === 'ready'; if (ready === this.ready) return; this.ready = ready; this.stopStream(); if (ready) void this.open() }
    this.unsubscribe = this.host.subscribe(changed); changed()
  }
  private stopStream() {
    ++this.generation; this.abort?.abort(); this.historyAbort?.abort(); this.stream = undefined; clearTimeout(this.timer)
    this.lease = undefined; this.leaseUntil = 0
    if (this.pendingInput) this.error = 'Input acknowledgement lost; input is not replayed'
    this.pendingInput = undefined; this.cursor = undefined; this.received = undefined; this.publish()
  }
  async open() {
    this.stopStream(); const generation = this.generation, abort = new AbortController(); this.abort = abort
    for (let attempt = 0; !abort.signal.aborted; attempt++) {
    const assembler = new TerminalFrameAssembler(uniqueId(), this.host.transport.native)
    try {
      const stream = await this.host.transport.stream('TerminalService/TerminalSession', event => this.apply(event, assembler, generation), abort.signal)
      if (generation !== this.generation) { stream.cancel(); return }
      this.stream = stream
      await stream.send({ subscribe: { terminal: this.terminal } })
      await stream.done
      if (!abort.signal.aborted) throw Error('Terminal subscription ended; reconnecting')
    } catch (e) {
      if (!abort.signal.aborted) {
        this.error = this.pendingInput ? 'Input acknowledgement lost; input is not replayed' : String(e)
        this.stream?.cancel(); this.stream = undefined; clearTimeout(this.timer)
        this.lease = undefined; this.leaseUntil = 0; this.pendingInput = undefined
        this.cursor = undefined; this.received = undefined; this.publish()
      }
    } finally { await assembler.reset() }
    await recoveryDelay(attempt, abort.signal)
    }
  }
  private async apply(event: TerminalServerEvent, assembler: TerminalFrameAssembler, generation: number) {
    if (generation !== this.generation || this.abort?.signal.aborted) return
    if (event.terminal?.host_id !== this.terminal.host_id || event.terminal.terminal_id !== this.terminal.terminal_id || !event.cursor) throw Error('Terminal identity missing')
    if (event.resync) throw Error(event.resync.reason)
    if (this.received) {
      if (event.cursor.runtime_epoch !== this.received.runtime_epoch || event.cursor.stream_id !== this.received.stream_id || BigInt(event.cursor.sequence) !== BigInt(this.received.sequence) + 1n || event.run_id !== this.runId) throw Error('Terminal sequence or run changed')
    }
    this.received = event.cursor
    if (event.frame_fragment) {
      const frame = await assembler.push(event.frame_fragment)
      if (generation !== this.generation || this.abort?.signal.aborted) return
      if (!frame) { if (!this.cursor) this.runId = event.run_id; return }
      event = { ...event, frame, frame_fragment: undefined }
    } else if (assembler.pending) throw Error('Interleaved terminal fragments')
    if (!this.cursor && !event.frame?.complete) throw Error('Terminal snapshot required')
    if (this.runId && this.runId !== event.run_id) { this.lease = undefined; this.pendingInput = undefined; clearTimeout(this.timer) }
    this.runId = event.run_id
    if (event.frame) {
      const f = event.frame
      if (!f.columns || !f.rows || f.columns * f.rows > 200000) throw Error('Terminal frame exceeds bounds')
      if (!f.complete && (!this.frame || f.columns !== this.frame.columns || f.rows !== this.frame.rows)) throw Error('Terminal dimensions require snapshot')
      const cells = new Map((f.complete ? [] : this.frame!.cells).map(c => [`${c.row}:${c.column}`, c]))
      for (const cell of f.cells) { if (cell.column >= f.columns || cell.row >= f.rows) throw Error('Cell out of bounds'); cells.set(`${cell.row}:${cell.column}`, cell) }
      this.frame = { ...f, cells: [...cells.values()] }; this.error = undefined
    }
    if (event.lease) this.setLease(event.lease)
    if (event.input_ack && event.input_ack.input_id === this.pendingInput) { this.pendingInput = undefined; if (!event.input_ack.queued) this.error = event.input_ack.error?.message ?? 'Input rejected' }
    if (event.error) this.error = event.error.message
    this.cursor = event.cursor; this.publish()
  }
  private setLease(lease: InputLease) {
    if (lease.terminal?.host_id !== this.terminal.host_id || lease.terminal.terminal_id !== this.terminal.terminal_id || lease.run_id !== this.runId) throw Error('Input lease identity mismatch')
    this.lease = lease; this.leaseUntil = Date.now() + lease.expires_in_ms; clearTimeout(this.timer)
    if (lease.held) this.timer = setTimeout(() => { void this.stream?.send({ renew: { terminal: this.terminal, run_id: lease.run_id, fencing_token: lease.fencing_token } }).catch(e => { this.error = String(e); this.lease = undefined; this.publish() }) }, Math.max(100, Math.min(5000, Math.floor(lease.expires_in_ms / 3))))
  }
  project() {
    const catalog = this.host.snapshot().catalog
    const terminal = catalog?.terminals.find(t => t.terminal?.terminal_id === this.terminal.terminal_id && t.terminal.host_id === this.terminal.host_id)
    const project = catalog?.projects.flatMap(p => p.checkouts).find(w => w.catalog_id === terminal?.project_catalog_id)?.project
    if (!project || project.host_id !== this.terminal.host_id) throw Error('Terminal project is unavailable; refresh the host catalog')
    return project
  }
  async acquire(takeOver: boolean) {
    const receipt = checkReceipt(await this.host.mutate('TerminalService/AcquireInput', { command: command(this.host.store), project: this.project(), terminal_id: this.terminal.terminal_id, take_over: takeOver }))
    if (!receipt.terminal?.lease) throw Error('Terminal lease is unavailable; reconcile the original operation')
    this.setLease(receipt.terminal.lease); this.publish()
  }
  async release() {
    const lease = this.lease; this.lease = undefined; clearTimeout(this.timer); this.publish()
    if (lease?.held) checkReceipt(await this.host.mutate('TerminalService/ReleaseInput', { command: command(this.host.store), project: this.project(), terminal_id: this.terminal.terminal_id, run_id: lease.run_id, fencing_token: lease.fencing_token }))
  }
  private inputLease() { if (!this.stream || !this.lease?.held || Date.now() >= this.leaseUntil) throw Error('Acquire terminal input first'); return this.lease }
  async input(text: string) {
    const lease = this.inputLease()
    if (this.pendingInput) throw Error('Previous input is awaiting acknowledgement')
    const data = new TextEncoder().encode(text); if (data.length > 65536) throw Error('Paste exceeds 64 KiB')
    const inputId = String(++this.inputId); this.pendingInput = inputId; this.publish()
    try { await this.stream!.send({ input: { terminal: this.terminal, run_id: lease.run_id, fencing_token: lease.fencing_token, input_id: inputId, data } }) }
    catch (e) { this.error = 'Input outcome unknown; never automatically replayed'; this.publish(); throw e }
  }
  async paste(text: string) {
    const clean = text.replace(/[\x00-\x08\x0b-\x1f\x7f]/g, '')
    const bracketed = (BigInt(this.frame?.input_modes ?? '0') & BigInt(TerminalInputMode.TERMINAL_INPUT_MODE_BRACKETED_PASTE)) !== 0n
    await this.input(bracketed ? `\x1b[200~${clean}\x1b[201~` : clean.replace(/\r?\n/g, '\r'))
  }
  async key(key: string, modifiers: KeyModifiers = {}) {
    return this.input(terminalKey(key, BigInt(this.frame?.input_modes ?? '0'), modifiers))
  }
  async earlier(offset = 0, more = false) {
    const runId = this.runId; if (!runId) throw Error('Terminal run unavailable')
    let start: TerminalHistoryStart | undefined, frame: TerminalFrame | undefined, ended = false
    this.historyAbort?.abort()
    const abort = new AbortController(); this.historyAbort = abort
    const assembler = new TerminalFrameAssembler(uniqueId(), this.host.transport.native)
    const generation = this.generation
    try {
      const stream = await this.host.transport.subscribe('TerminalService/ReadHistory', { terminal: this.terminal, run_id: runId, offset_from_bottom: offset, rows: 200, page_token: more ? this.history?.start.previous_page_token : undefined }, async event => {
        if (ended || abort.signal.aborted || generation !== this.generation || event.terminal?.host_id !== this.terminal.host_id || event.terminal.terminal_id !== this.terminal.terminal_id || event.run_id !== runId || this.runId !== runId) throw Error('History identity changed')
        if (event.frame_fragment) {
          if (!start) throw Error('Missing history start')
          const decoded = await assembler.push(event.frame_fragment)
          if (!decoded) return
          event = { ...event, frame: decoded, frame_fragment: undefined }
        } else if (assembler.pending) throw Error('Interleaved history fragments')
        if (event.start) { if (start || !event.start.boundary) throw Error('Invalid terminal history start'); start = event.start }
        else if (event.frame) {
          const f = event.frame
          if (!start || !f.rows || !f.columns || f.rows > 200 || f.rows * f.columns > 200000) throw Error('Invalid history frame')
          if (!f.complete && (!frame || frame.columns !== f.columns || frame.rows !== f.rows)) throw Error('History snapshot required')
          const cells = new Map((f.complete ? [] : frame!.cells).map(c => [c.row + ':' + c.column, c]))
          for (const cell of f.cells) {
            if (cell.row >= f.rows || cell.column >= f.columns) throw Error('History cell out of bounds')
            cells.set(cell.row + ':' + cell.column, cell)
          }
          frame = { ...f, cells: [...cells.values()] }
        } else if (event.end) {
          if (!start?.boundary || !frame || event.end.runtime_epoch !== start.boundary.runtime_epoch || event.end.stream_id !== start.boundary.stream_id || event.end.sequence !== start.boundary.sequence) throw Error('Terminal history boundary mismatch')
          ended = true
        } else throw Error('Invalid history event')
      }, abort.signal)
      await stream.done
      if (!ended || !start || !frame || abort.signal.aborted || this.runId !== runId || generation !== this.generation) throw Error('Incomplete terminal history')
    } finally { await assembler.reset() }
    this.history = { frame, start }; this.publish()
  }
  live() { this.historyAbort?.abort(); this.history = undefined; this.publish() }
  async resize(columns: number, rows: number) { const lease = this.inputLease(); await this.stream!.send({ resize: { terminal: this.terminal, run_id: lease.run_id, fencing_token: lease.fencing_token, columns, rows } }) }
  async dispose() { this.unsubscribe?.(); this.ready = false; const releasing = this.release(); this.stopStream(); this.listeners.clear(); await releasing }
}
