const { test } = require('node:test')
const assert = require('node:assert/strict')
const { execFileSync } = require('node:child_process')
const { resolve } = require('node:path')
const root = process.env.CANOPY_TEST_BUILD
if (!root) throw Error('Run using scripts/test-remote.mjs')
const { encode, decode, toBase64 } = require(resolve(root, 'protobuf.js'))
const { HostStore } = require(resolve(root, 'persistence.js'))
const { ClientOutbox } = require(resolve(root, 'outbox.js'))
const { GrpcTransport } = require(resolve(root, 'transport.js'))
const connection = { hostId: 'host-a', deviceId: 'device-a', name: 'A', serverName: 'canopy.local', fingerprint: 'a'.repeat(64), hostCertificate: 'test', credentialRef: 'credential-a', routes: [{ kind: 'direct', address: '127.0.0.1', port: 1234 }], route: 0 }
const context = { account_id: 'acp', history_epoch: '7' }
const target = { thread: { host_id: 'host-a', thread_id: 'thread-a' }, executor_epoch: '1' }
const catalogThread = context => ({ catalog_id: 'chat-a', thread: target.thread, summary: { agent_id: context.account_id, profile_id: context.profile_id, history_epoch: context.history_epoch } })
const draft = { text: 'Do work', attachments: [], goal: false }
const entry = (id = 'command-a') => ({ commandId: id, messageId: 'message-' + id, hostId: 'host-a', deviceId: 'device-a', chatId: 'chat-a', context, draft, createdAt: '2026-10-25T00:00:00Z' })
function storage() {
  const files = new Map()
  return { fail: false, files, async readState(k) { return files.get(k) ?? null }, async writeState(k, v) { if (this.fail) throw Error('disk full'); files.set(k, v) } }
}
function server() {
  const effects = [], requests = []
  return {
    effects, requests, loseSend: false, loseClose: false, epoch: '2', chatContext: context,
    async subscribe(method, request, onMessage) {
      assert.equal(method, 'CatalogService/ReadCatalog')
      requests.push({ method, request: structuredClone(request) })
      const cursor = { runtime_epoch: 'runtime', stream_id: 'catalog', sequence: '1' }
      const page = { environment: { host_id: 'host-a' }, projects: [], terminals: [], threads: [catalogThread(this.chatContext)] }
      const done = Promise.resolve().then(() => onMessage({ cursor, snapshot_part: { boundary: cursor, page, first: true, last: true } }))
      return { done, cancel() {}, async send() {}, async finish() {} }
    },
    async unary(method, request) {
      requests.push({ method, request: structuredClone(request) })
      if (method === 'HostService/GetInfo') return { environment: { host_id: 'host-a' }, protocol_version: 1, command_epoch: this.epoch }
      if (method === 'CatalogService/ListThreads') return { threads: [{ target }] }
      if (method === 'ThreadService/SendMessage') {
        if (!effects.includes(request.command.command_id)) effects.push(request.command.command_id)
        if (this.loseSend) throw Error('response lost')
        return { host_id: 'host-a', ...request.command, state: 3 }
      }
      if (method === 'HostService/GetCommand') return effects.includes(request.command_id) ? { state: 1, receipt: { host_id: 'host-a', command_id: request.command_id, command_epoch: request.command_epoch, state: 3 } } : { state: 2 }
      if (method === 'HostService/CloseCommandEpoch') {
        this.epoch = String(BigInt(request.expected_epoch) + 1n)
        if (this.loseClose) throw Error('close response lost')
        return { environment: { host_id: 'host-a' }, epoch: this.epoch }
      }
      throw Error(method)
    },
  }
}

test('protobuf encoding interoperates with protoc: unicode, max uint64, optional empty and binary', () => {
  const object = { command: { command_id: 'c', command_epoch: '18446744073709551615' }, target: { thread: { host_id: 'h', thread_id: 'čat' }, executor_epoch: '1' }, message_id: 'm', draft: { text: '你好 🧑‍💻', attachments: [], goal: true }, expected_context: { account_id: 'acp', profile_id: '', history_epoch: '123' } }
  const bytes = encode('SendMessageRequest', object)
  const decoded = execFileSync('protoc', ['-I', '../proto', '--decode=canopy.api.v1.SendMessageRequest', 'canopy/api/v1/api.proto'], { cwd: resolve(__dirname, '..'), input: bytes }).toString()
  assert.match(decoded, /18446744073709551615/)
  assert.match(decoded, /profile_id: ""/)
  assert.deepEqual(decode('SendMessageRequest', bytes), object)
  const file = { environment: { host_id: 'h' }, upload_id: 'u', offset: '999', data: Uint8Array.from([0, 255, 128, 10]) }
  assert.deepEqual(decode('PutChunkRequest', encode('PutChunkRequest', file)), file)
  assert.throws(() => decode('PutChunkRequest', Uint8Array.from([34, 128])), /truncated/)
  assert.throws(() => encode('TerminalSessionRequest', { input: {}, resize: {} }), /oneof/)
})
test('outbox persists envelope before send; disk failure causes no effect and preserves draft', async () => {
  const io = storage(), store = await HostStore.open(io, connection), outbox = new ClientOutbox(store), remote = server()
  await outbox.saveDraft('chat-a', context, draft)
  io.fail = true
  await assert.rejects(outbox.enqueue(entry()), /disk full/)
  assert.equal(store.snapshot().drafts['chat-a'].draft.text, draft.text)
  io.fail = false
  await outbox.enqueue(entry())
  io.fail = true
  await assert.rejects(outbox.sendExplicit('command-a', remote), /disk full/)
  assert.equal(remote.effects.length, 0)
  assert.equal(store.snapshot().outbox[0].epoch, undefined)
})
test('contextual drafts survive rejected offline enqueue and restart without network effects', async () => {
  const io = storage(), remote = server(), outbox = new ClientOutbox(await HostStore.open(io, connection))
  const contextual = { ...draft, attachments: [{ label: 'Quote', quote: { text: 'Original context' } }] }
  await outbox.saveDraft('chat-a', context, contextual)
  await assert.rejects(outbox.enqueue({ ...entry(), draft: contextual }), /online send/)
  const restored = new ClientOutbox(await HostStore.open(io, connection))
  await restored.reconcile(remote)
  assert.deepEqual(restored.store.snapshot().drafts['chat-a'].draft, contextual)
  assert.equal(restored.store.snapshot().outbox.length, 0)
  assert.equal(remote.effects.length, 0)
})
test('lost send response survives restart; reconnect queries result without repeating effects', async () => {
  const io = storage(), remote = server()
  let outbox = new ClientOutbox(await HostStore.open(io, connection))
  await outbox.enqueue(entry()); remote.loseSend = true
  await assert.rejects(outbox.sendExplicit('command-a', remote), /response lost/)
  outbox = new ClientOutbox(await HostStore.open(io, connection))
  await outbox.enqueue(entry('offline-b'))
  await outbox.reconcile(remote)
  assert.deepEqual(remote.effects, ['command-a'])
  assert.equal(outbox.store.snapshot().outbox[1].epoch, undefined)
  assert.equal(outbox.store.snapshot().outbox[0].receipt.state, 3)
})
test('not_found retains old command ID; account changes reject unsent text', async () => {
  const io = storage(), remote = server(), outbox = new ClientOutbox(await HostStore.open(io, connection))
  await outbox.enqueue(entry())
  remote.chatContext = { ...context, profile_id: 'other' }
  await assert.rejects(outbox.sendExplicit('command-a', remote), /account changed/)
  assert.deepEqual(remote.effects, [])
  remote.chatContext = context
  remote.loseSend = true
  await assert.rejects(outbox.sendExplicit('command-a', remote))
  remote.effects.length = 0 // Simulate request never reaching host rather than lost response.
  remote.loseSend = false
  await assert.rejects(outbox.reconcile(remote), /not found/)
  assert.deepEqual(remote.effects, [])
  await outbox.sendExplicit('command-a', remote)
  const sends = remote.requests.filter(r => r.method === 'ThreadService/SendMessage')
  assert.deepEqual(sends[0].request, sends[1].request)
})
test('lost Close response blocks new envelopes, retry is same epoch and retains unsent text', async () => {
  const io = storage(), remote = server(), outbox = new ClientOutbox(await HostStore.open(io, connection))
  await outbox.enqueue(entry()); await outbox.sendExplicit('command-a', remote)
  await outbox.enqueue(entry('offline-b'))
  remote.loseClose = true
  await assert.rejects(outbox.closeEpoch(remote), /close response lost/)
  await assert.rejects(outbox.sendExplicit('offline-b', remote), /closure is unresolved/)
  remote.loseClose = false
  const restored = new ClientOutbox(await HostStore.open(io, connection))
  await restored.closeEpoch(remote)
  assert.deepEqual(remote.requests.filter(r => r.method === 'HostService/CloseCommandEpoch').map(r => r.request.expected_epoch), ['2', '2'])
  assert.equal(restored.store.snapshot().outbox[0].commandId, 'offline-b')
  assert.equal(restored.store.snapshot().outbox[0].epoch, undefined)
})
test('reviewed unknown command survives lost review response and epoch closure without replay', async () => {
  const io = storage(), store = await HostStore.open(io, connection)
  await store.update(state => { state.epoch = '2'; state.mutations = [{ method: 'GitService/Push', commandId: 'uncertain', epoch: '2' }]; return null })
  let reviewed = false, reviewCalls = 0, closeCalls = 0
  const remote = { async unary(method, request) {
    if (method === 'HostService/GetCommand') return { state: 1, receipt: { host_id: 'host-a', command_id: request.command_id, command_epoch: request.command_epoch, state: 6, resolved_by_user: reviewed } }
    if (method === 'HostService/ResolveCommand') { reviewed = true; reviewCalls++; throw Error('review reply lost') }
    if (method === 'HostService/CloseCommandEpoch') { assert.equal(reviewed, true); closeCalls++; return { environment: { host_id: 'host-a' }, epoch: '3' } }
    throw Error('Unexpected effect replay: ' + method)
  } }
  await assert.rejects(new ClientOutbox(store).closeEpoch(remote, true), /review reply lost/)
  assert.equal(closeCalls, 0)
  const restored = await HostStore.open(io, connection)
  await new ClientOutbox(restored).closeEpoch(remote, true)
  assert.equal(reviewCalls, 1)
  assert.equal(closeCalls, 1)
  assert.equal(restored.snapshot().mutations[0].receipt.state, 6)
  assert.equal(restored.snapshot().mutations[0].receipt.resolved_by_user, true)
})
test('transport ACK follows consumer apply; cancellation rejects pending write and ignores late frame', async () => {
  let listener, applied = false, acked = false, release
  const native = {
    addListener(_, f) { listener = f; return { remove() {} } },
    async start(_, id) { this.id = id }, async connect() {}, async disconnect() {},
    async write() {}, async finish() {}, async cancel() {},
    async ack() { assert.equal(applied, true); acked = true },
  }
  const transport = new GrpcTransport(native, 'host')
  const call = await transport.stream('HostService/WatchHost', async () => { await new Promise(resolve => { release = resolve }); applied = true })
  listener({ id: native.id, kind: 'message', data: toBase64(encode('HostStatus', { host_id: 'h', readiness: 2, reason: '' })) })
  await new Promise(resolve => setImmediate(resolve))
  assert.equal(acked, false); release()
  await new Promise(resolve => setImmediate(resolve)); assert.equal(acked, true)
  const write = call.send({ host_id: 'h' })
  call.cancel()
  await assert.rejects(write, /cancelled/)
  await assert.rejects(call.done, /cancelled/)
  listener({ id: native.id, kind: 'message', data: 'bad bytes' })
  await transport.dispose()
})

const { createHash } = require('node:crypto')
const { ChatEventAssembler } = require(resolve(root, 'fragments.js'))
function hashes() {
  const active = new Map()
  return {
    async startHash(id) { active.set(id, createHash('sha256')) },
    async hashChunk(id, data) { active.get(id).update(Buffer.from(data, 'base64')) },
    async finishHash(id) { const h = active.get(id); active.delete(id); return h.digest('base64') },
    async cancelHash(id) { active.delete(id) },
  }
}
function fragments() {
  const item = { key: { history_epoch: '7', item_id: '9' }, revision: '3', turn_id: '1', trimmed: false, text: { text: '你好'.repeat(18000), streaming: true } }
  const bytes = encode('TranscriptItem', item)
  const digest = Uint8Array.from(createHash('sha256').update(bytes).digest())
  const result = []
  for (let offset = 0; offset < bytes.length; offset += 65536) result.push({ thread: { host_id: 'h', thread_id: 'c' }, cursor: { runtime_epoch: 'r', stream_id: 's', sequence: String(11 + result.length) }, item_fragment: { key: item.key, revision: item.revision, transfer_id: 'transfer', offset: String(offset), total_bytes: String(bytes.length), sha256: digest, data: bytes.slice(offset, offset + 65536) } })
  return { item, events: result }
}
test('fragment assembly commits cursor only after complete verified item; cancel discards partial', async () => {
  const since = { runtime_epoch: 'r', stream_id: 's', sequence: '10' }
  const a = new ChatEventAssembler({ host_id: 'h', thread_id: 'c' }, 'subscription', hashes(), since)
  const { item, events } = fragments(), installed = []
  await a.push(events[0], value => installed.push(value))
  assert.deepEqual(a.resumeCursor(), since)
  assert.equal(installed.length, 0)
  await a.reset()
  for (const event of events) await a.push(event, value => installed.push(value))
  assert.deepEqual(installed[0].replace_item, item)
  assert.equal(a.resumeCursor().sequence, events.at(-1).cursor.sequence)
})
test('bad fragment hash and live gap never expose partial replacement or advance resumable cursor', async () => {
  const since = { runtime_epoch: 'r', stream_id: 's', sequence: '10' }
  const a = new ChatEventAssembler({ host_id: 'h', thread_id: 'c' }, 'subscription', hashes(), since)
  const { events } = fragments(), installed = []
  events.at(-1).item_fragment.data[2] ^= 1
  await a.push(events[0], value => installed.push(value))
  await assert.rejects(a.push(events[1], value => installed.push(value)), /digest mismatch/)
  assert.equal(installed.length, 0)
  assert.deepEqual(a.resumeCursor(), since)
  await assert.rejects(a.push({ ...events[0], cursor: { ...since, sequence: '12' } }, () => {}), /sequence gap/)
  assert.deepEqual(a.resumeCursor(), since)
})

const { ChatProjectionOwner, HistoryPageAssembler } = require(resolve(root, 'projection.js'))
test('snapshot installation is atomic; interrupted replacement retains old projection and UTF8 tail validates offsets', async () => {
  const owner = new ChatProjectionOwner(), chat = { host_id: 'h', thread_id: 'c' }, boundary = { runtime_epoch: 'r', stream_id: 's', sequence: '10' }
  const assembler = new ChatEventAssembler(chat, 'view', hashes())
  const item = { key: { history_epoch: '1', item_id: '1' }, revision: '1', turn_id: '1', trimmed: false, text: { text: '你', streaming: true } }
  const push = value => assembler.push({ thread: chat, ...value }, event => owner.apply(event))
  await push({ snapshot_start: { boundary, saved_sequence: '0' } })
  await push({ snapshot_items: { items: [item] } })
  assert.equal(owner.snapshot().items.length, 0)
  await push({ snapshot_end: { boundary, has_earlier_history: false } })
  await push({ cursor: { ...boundary, sequence: '11' }, append_text: { key: item.key, expected_revision: '1', revision: '2', utf8_offset: '3', text: '好', streaming: false } })
  assert.equal(owner.snapshot().items[0].text.text, '你好')
  await push({ snapshot_start: { boundary: { ...boundary, sequence: '20' }, saved_sequence: '0' } })
  await push({ snapshot_items: { items: [{ ...item, text: { text: 'uncommitted', streaming: false } }] } })
  await assembler.reset(); owner.discardSnapshot()
  assert.equal(owner.snapshot().items[0].text.text, '你好')
  assert.throws(() => owner.apply({ append_text: { key: item.key, expected_revision: '2', revision: '3', utf8_offset: '2', text: 'bad', streaming: false } }), /offset conflict/)
})
test('fragmented history page is unavailable until matching end boundary', async () => {
  const chat = { host_id: 'h', thread_id: 'c' }, page = new HistoryPageAssembler(chat, 'history', hashes()), { item, events } = fragments()
  await page.push({ thread: chat, start: { read_id: 'read', previous_page_token: 'older', incomplete: false } })
  for (const event of events) await page.push({ thread: chat, fragment: event.item_fragment })
  assert.throws(() => page.result(), /Incomplete/)
  await assert.rejects(page.push({ thread: chat, end_read_id: 'wrong' }), /boundary/)
  await page.push({ thread: chat, end_read_id: 'read' })
  assert.deepEqual(page.result().items, [item])
  await page.dispose()
})
const { upload, prepareUpload } = require(resolve(root, 'uploads.js'))
test('upload retries lost Begin and Finish responses using immutable IDs and reconciles chunks after restart', async () => {
  const io = storage(), store = await HostStore.open(io, connection)
  await store.update(state => { state.epoch = '2' })
  const bytes = Buffer.from('image bytes'), digest = Uint8Array.from(createHash('sha256').update(bytes).digest()), calls = []
  let loseBegin = true, loseFinish = true, received = false, ready = false
  const artifact = { host_id: 'host-a', artifact_id: 'artifact', size: String(bytes.length), sha256: digest, media_type: 'image/png' }
  const transport = {
    native: { async readAttachment(id, offset, length) { return bytes.subarray(offset, offset + length).toString('base64') } },
    async unary(method, request) {
      calls.push({ method, request: structuredClone(request) })
      if (method.endsWith('/BeginImageUpload')) { if (loseBegin) { loseBegin = false; throw Error('lost begin response') }; return { host_id: 'host-a', ...request.command, state: 3, artifact: { upload_id: 'upload' } } }
      if (method.endsWith('/PutChunk')) { assert.equal(received, false); assert.deepEqual(Buffer.from(request.data), bytes); received = true }
      if (method.endsWith('/FinishImageUpload')) { ready = true; if (loseFinish) { loseFinish = false; throw Error('lost finish response') }; return { host_id: 'host-a', ...request.command, state: 3 } }
      return { environment: { host_id: 'host-a' }, upload_id: 'upload', received: received ? [{ offset: '0', length: String(bytes.length) }] : [], artifact: ready ? artifact : undefined }
    },
  }
  let host = { store, transport, snapshot: () => ({ hostId: 'host-a' }) }
  const journal = await prepareUpload(host, { image: target }, 'chat-image', 'source', bytes.length, digest, 'image/png')
  await assert.rejects(upload(host, journal.id), /lost begin/)
  host = { store: await HostStore.open(io, connection), transport, snapshot: host.snapshot }
  await assert.rejects(upload(host, journal.id), /lost finish/)
  host = { store: await HostStore.open(io, connection), transport, snapshot: host.snapshot }
  assert.deepEqual(await upload(host, journal.id), artifact)
  const begins = calls.filter(c => c.method.endsWith('/BeginImageUpload'))
  assert.deepEqual(begins[0].request, begins[1].request)
  assert.equal(calls.filter(c => c.method.endsWith('/PutChunk')).length, 1)
  assert.equal(calls.filter(c => c.method.endsWith('/FinishImageUpload')).length, 1)
})

test('standard protobuf Struct maps preserve nested typed provider fields and hostile map keys', () => {
  const value = { fields: Object.fromEntries([['count', { number_value: 2.5 }], ['enabled', { bool_value: false }], ['__proto__', { string_value: 'ordinary key' }], ['list', { list_value: { values: [{ null_value: 0 }, { string_value: '你好' }] } }]]) }
  const bytes = encode('Struct', value)
  const decoded = execFileSync('protoc', ['--decode=google.protobuf.Struct', 'google/protobuf/struct.proto'], { input: bytes }).toString()
  assert.match(decoded, /number_value: 2.5/)
  assert.deepEqual(decode('Struct', bytes), value)
  assert.equal({}.polluted, undefined)
})
const { TerminalViewOwner } = require(resolve(root, 'terminal.js'))
test('terminal sends fenced mode-aware input and never repeats bytes after lost acknowledgement', async () => {
  const io = storage(), store = await HostStore.open(io, connection); await store.update(s => { s.epoch = '2' })
  let receive, resolveStream; const sent = []
  const host = { store, snapshot: () => ({ hostId: 'host-a', phase: 'ready', catalog: { terminals: [{ terminal: { host_id: 'host-a', terminal_id: 'term' }, project_catalog_id: 'w' }], projects: [{ checkouts: [{ catalog_id: 'w', project: { host_id: 'host-a', project_id: 'project-a', worktree_id: 'worktree-a' } }] }] } }), subscribe: () => () => {}, transport: {
    async stream(method, callback) { receive = callback; return { send: async value => { sent.push(value) }, cancel() {}, done: new Promise(resolve => { resolveStream = resolve }) } },
    async unary() { throw Error('Unexpected read') },
  } }
  host.mutate = async (method, request) => ({ host_id: 'host-a', ...request.command, state: 3, terminal: { terminal_id: 'term', run_id: 'run', lease: { terminal: { host_id: 'host-a', terminal_id: 'term' }, run_id: 'run', fencing_token: '91', expires_in_ms: 15000, held: !method.endsWith('ReleaseInput') } } })
  const owner = new TerminalViewOwner(host, 'term'); owner.start(); await new Promise(resolve => setImmediate(resolve))
  const terminal = { host_id: 'host-a', terminal_id: 'term' }
  receive({ terminal, run_id: 'run', cursor: { runtime_epoch: 'epoch', stream_id: 'term', sequence: '1' }, frame: { columns: 80, rows: 24, cells: [], cursor_column: 0, cursor_row: 0, input_modes: '5', lost_scrollback: false, complete: true } })
  await owner.acquire(false); await owner.key('up')
  assert.equal(new TextDecoder().decode(sent.at(-1).input.data), '\x1bOA')
  assert.equal(sent.at(-1).input.fencing_token, '91')
  await assert.rejects(owner.input('again'), /awaiting acknowledgement/)
  const inputId = sent.at(-1).input.input_id
  receive({ terminal, run_id: 'run', cursor: { runtime_epoch: 'epoch', stream_id: 'term', sequence: '2' }, input_ack: { input_id: inputId, queued: true } })
  await owner.paste('hello\nworld')
  assert.equal(new TextDecoder().decode(sent.at(-1).input.data), '\x1b[200~hello\nworld\x1b[201~')
  const before = sent.length; await owner.dispose(); resolveStream(); assert.equal(sent.length, before)
})

test('terminal fragmented viewport installs atomically, patches cells, and rejects corrupt history', async () => {
  let receive, resolveStream, historyEvents = []
  const terminal = { host_id: 'host-a', terminal_id: 'term' }
  const cursor = sequence => ({ runtime_epoch: 'epoch', stream_id: 'term', sequence: String(sequence) })
  const host = { snapshot: () => ({ hostId: 'host-a', phase: 'ready' }), subscribe: () => () => {}, transport: {
    native: hashes(),
    async stream(method, callback) { receive = callback; return { send: async () => {}, cancel() {}, done: new Promise(resolve => { resolveStream = resolve }) } },
    async subscribe(method, request, callback) { return { done: (async () => { for (const event of historyEvents) await callback(event) })() } },
  } }
  const owner = new TerminalViewOwner(host, 'term'); owner.start(); await new Promise(resolve => setImmediate(resolve))
  const frame = { columns: 80, rows: 24, cells: [{ row: 0, column: 0, text: 'a'.repeat(70000), foreground: 0, background: 0, flags: 0, column_span: 1 }], cursor_column: 0, cursor_row: 0, input_modes: '0', lost_scrollback: false, complete: true }
  const bytes = encode('TerminalFrame', frame)
  const sha256 = Uint8Array.from(createHash('sha256').update(bytes).digest())
  const fragment = offset => ({ frame_id: 'frame', offset: String(offset), total_bytes: String(bytes.length), sha256, data: bytes.slice(offset, offset + 65536) })
  await receive({ terminal, run_id: 'run', cursor: cursor(1), frame_fragment: fragment(0) })
  assert.equal(owner.snapshot().frame, undefined)
  assert.equal(owner.cursor, undefined)
  await receive({ terminal, run_id: 'run', cursor: cursor(2), frame_fragment: fragment(65536) })
  assert.equal(owner.snapshot().frame.cells[0].text.length, 70000)
  assert.equal(owner.cursor.sequence, '2')
  await receive({ terminal, run_id: 'run', cursor: cursor(3), frame: { ...frame, complete: false, cells: [{ ...frame.cells[0], column: 1, text: 'b' }] } })
  assert.equal(owner.snapshot().frame.cells.length, 2)
  await receive({ terminal, run_id: 'run', cursor: cursor(4), frame: { ...frame, cells: [] } })
  assert.equal(owner.snapshot().frame.cells.length, 0)
  const start = { boundary: cursor(4), offset_from_bottom: '0', retained_rows: '24', lost_scrollback: false }
  historyEvents = [
    { terminal, run_id: 'run', start },
    { terminal, run_id: 'run', frame_fragment: fragment(0) },
    { terminal, run_id: 'run', frame_fragment: fragment(65536) },
    { terminal, run_id: 'run', end: cursor(4) },
  ]
  await owner.earlier()
  const installed = owner.snapshot().history
  assert.equal(installed.frame.cells[0].text.length, 70000)
  historyEvents[2] = { ...historyEvents[2], frame_fragment: { ...fragment(65536), data: new Uint8Array(fragment(65536).data.length) } }
  await assert.rejects(owner.earlier(), /digest mismatch/)
  assert.equal(owner.snapshot().history, installed)
  await owner.dispose(); resolveStream()
})

const { ChatViewOwner } = require(resolve(root, 'chat.js'))
const flush = () => new Promise(resolve => setImmediate(resolve))
test('chat and terminal recover after more than three transient read failures and disposal cancels retry', async t => {
  t.mock.timers.enable({ apis: ['setTimeout'] })
  for (const kind of ['chat', 'terminal']) {
    let attempts = 0, stop
    const host = { snapshot: () => ({ hostId: 'host-a', phase: 'ready', catalog: { threads: [{ catalog_id: 'c', thread: { host_id: 'host-a', thread_id: 'c' } }] } }), subscribe: () => () => {}, transport: {
      native: hashes(),
      async subscribe(method, request, receive, signal) {
        attempts++
        if (attempts <= 5) throw Error('transient overflow')
        const thread = request.thread, boundary = { runtime_epoch: 'e', stream_id: 's', sequence: '1' }
        await receive({ thread, snapshot_start: { boundary, saved_sequence: '0' } })
        await receive({ thread, snapshot_end: { boundary, has_earlier_history: false } })
        await receive({ thread, cursor: { ...boundary, sequence: '2' }, replace_item: { key: { history_epoch: '1', item_id: '1' }, revision: '1', turn_id: '1', trimmed: false, text: { text: 'recovered', streaming: false } } })
        return { done: new Promise(resolve => { stop = resolve; signal.addEventListener('abort', resolve, { once: true }) }) }
      },
      async stream(method, receive, signal) {
        attempts++
        if (attempts <= 5) throw Error('transient overflow')
        return { send: async () => receive({ terminal: { host_id: 'host-a', terminal_id: 't' }, run_id: 'run', cursor: { runtime_epoch: 'e', stream_id: 's', sequence: '1' }, frame: { columns: 1, rows: 1, cells: [], cursor_column: 0, cursor_row: 0, input_modes: '0', complete: true, lost_scrollback: false } }), cancel() {}, done: new Promise(resolve => { stop = resolve; signal.addEventListener('abort', resolve, { once: true }) }) }
      },
    } }
    const owner = kind === 'chat' ? new ChatViewOwner(host, 'c') : new TerminalViewOwner(host, 't')
    owner.start(); await flush()
    for (let i = 0; i < 6; i++) { t.mock.timers.tick(2000); await flush() }
    assert.equal(attempts, 6)
    if (kind === 'chat') assert.equal(owner.snapshot().items[0].text.text, 'recovered')
    else assert.equal(owner.snapshot().frame.columns, 1)
    await owner.dispose(); stop?.(); await flush(); t.mock.timers.tick(100000); await flush()
    assert.equal(attempts, 6)
  }
})
const { reloadTextFile } = require(resolve(root, 'files.js'))
test('reload preserves durable draft on failed, cancelled or concurrent reads and atomically installs success', async () => {
  const io = storage(), store = await HostStore.open(io, connection), key = 'file:w:note.txt'
  const file = { target: { project: { host_id: 'host-a', project_id: 'project-a', worktree_id: 'worktree-a' } }, relative_path: 'note.txt' }
  await store.update(state => { state.views = { [key]: { text: 'unsaved draft', baseHash: 'old' } } })
  let fail = true, complete = true
  const host = { store, transport: { native: hashes(), async subscribe(method, request, callback) {
    return { done: (async () => { if (fail) throw Error('read failed'); await callback({ file, offset: '0', base_hash: Uint8Array.from(createHash('sha256').update('host text').digest()), data: new TextEncoder().encode('host text'), complete }) })() }
  } } }
  const abort = new AbortController()
  await assert.rejects(reloadTextFile(host, file, abort.signal, () => true), /read failed/)
  fail = false; complete = false
  await assert.rejects(reloadTextFile(host, file, abort.signal, () => true), /Incomplete/)
  complete = true
  await assert.rejects(reloadTextFile(host, file, abort.signal, () => false), /buffer changed/)
  abort.abort()
  await assert.rejects(reloadTextFile(host, file, abort.signal, () => true), /cancelled/)
  assert.equal((await HostStore.open(io, connection)).snapshot().views[key].text, 'unsaved draft')
  io.fail = true
  await assert.rejects(reloadTextFile(host, file, new AbortController().signal, () => true), /disk full/)
  io.fail = false
  assert.equal(store.snapshot().views[key].text, 'unsaved draft')
  await reloadTextFile(host, file, new AbortController().signal, () => true, key)
  assert.equal((await HostStore.open(io, connection)).snapshot().views[key].text, 'host text')
})
const { terminalKey } = require(resolve(root, 'terminal-keys.js'))
test('terminal navigation, function and hardware modifiers produce TUI sequences in the current mode', () => {
  assert.equal(terminalKey('home', 1n), '\x1bOH')
  assert.equal(terminalKey('end', 0n, { ctrl: true }), '\x1b[1;5F')
  assert.equal(terminalKey('f12', 0n, { shift: true, alt: true }), '\x1b[24;4~')
  assert.equal(terminalKey('f1', 0n), '\x1bOP')
  assert.equal(terminalKey('pageup', 0n), '\x1b[5~')
  assert.equal(terminalKey('delete', 0n), '\x1b[3~')
  assert.equal(terminalKey('backspace', 0n), '\x7f')
  assert.equal(terminalKey('c', 0n, { ctrl: true }), '\x03')
  assert.equal(terminalKey('x', 0n, { alt: true }), '\x1bx')
  assert.equal(terminalKey('tab', 0n, { shift: true }), '\x1b[Z')
  assert.equal(terminalKey('enter', 32n), '\r\n')
})

const { chatRequest, retryChatMutation, restoreChatRequest } = require(resolve(root, 'chat-commands.js'))
test('explicit chat recovery preserves original request and command ID across lost replies; unknown is not replayed', async () => {
  const io = storage(), store = await HostStore.open(io, connection)
  const request = { command: { command_id: 'edit-original', command_epoch: '2' }, target, run_id: '1', item: { history_epoch: '1', item_id: '4' }, draft: { text: 'replacement', attachments: [{ label: 'quote', quote: { text: 'original context' } }], goal: false }, restore_files: true }
  await store.update(state => { state.epoch = '2'; state.mutations = [{ method: 'ThreadService/EditMessage', commandId: request.command.command_id, epoch: '2', chatRequest: chatRequest('ThreadService/EditMessage', request) }] })
  let sends = 0, status = 2, lost = true
  const transport = { async unary(method, body) {
    if (method.endsWith('/GetCommand')) return status === 2 ? { state: 2 } : { state: 1, receipt: { host_id: 'host-a', ...request.command, state: status } }
    assert.deepEqual(body, request)
    sends++
    if (lost) { lost = false; throw Error('lost reply') }
    return { host_id: 'host-a', ...request.command, state: 3 }
  } }
  await assert.rejects(retryChatMutation(store, transport, 'edit-original'), /lost reply/)
  const restored = await HostStore.open(io, connection)
  await retryChatMutation(restored, transport, 'edit-original')
  assert.equal(sends, 2)
  status = 6
  assert.equal((await retryChatMutation(restored, transport, 'edit-original')).state, 6)
  assert.equal(sends, 2)
  await restored.update(state => { state.epoch = '3' }); status = 2
  await assert.rejects(retryChatMutation(restored, transport, 'edit-original'), /retired/)
  assert.equal(sends, 2)
  assert.deepEqual(restoreChatRequest('ThreadService/EditMessage', restored.snapshot().mutations[0].chatRequest), request)
})

const { readChatPage } = require(resolve(root, 'artifact-preview.js'))
test('interactive chat page validates authorized artifact and cleans corrupted or interrupted private bytes', async () => {
  const bytes = Buffer.from('<html><script>document.body.textContent="page"</script></html>')
  const artifact = { host_id: 'host-a', artifact_id: 'page', size: String(bytes.length), sha256: Uint8Array.from(createHash('sha256').update(bytes).digest()), media_type: 'text/html' }
  const files = new Map(); let corrupt = false, finish = true
  const host = { snapshot: () => ({ hostId: 'host-a' }), transport: {
    native: {
      async beginAttachment(id) { files.set(id, Buffer.alloc(0)) },
      async appendAttachment(id, data) { files.set(id, Buffer.concat([files.get(id), Buffer.from(data, 'base64')])) },
      async finishAttachment(id) { return { size: files.get(id).length, sha256: createHash('sha256').update(files.get(id)).digest('base64') } },
      async readAttachment(id, offset, length) { return files.get(id).subarray(offset, offset + length).toString('base64') },
      async attachmentUri(id) { return `file:///private/${id}` },
      async removeAttachment(id) { files.delete(id) },
    },
    async subscribe(method, request, receive) { return { done: receive({ artifact: { ...artifact, sha256: corrupt ? new Uint8Array(32) : artifact.sha256 }, offset: '0', data: bytes, complete: finish }) } },
  } }
  const chat = { host_id: 'host-a', thread_id: 'c' }, item = { history_epoch: '1', item_id: '2' }
  corrupt = true
  await assert.rejects(readChatPage(host, chat, item), /checksum/); assert.equal(files.size, 0)
  corrupt = false; finish = false
  await assert.rejects(readChatPage(host, chat, item), /Incomplete/); assert.equal(files.size, 0)
  finish = true
  const page = await readChatPage(host, chat, item)
  assert.equal(page.html, bytes.toString()); assert.equal(files.size, 1)
  await host.transport.native.removeAttachment(page.id)
})

test('returning to live history cancels an older page even when transport finishes late', async () => {
  let finish, signal
  const chat = { host_id: 'h', thread_id: 'c' }
  const host = { snapshot: () => ({ hostId: 'h', catalog: { threads: [{ catalog_id: 'c', thread: chat }] } }), transport: { native: hashes(), async subscribe(method, request, receive, cancellation) {
    signal = cancellation
    await receive({ thread: chat, start: { read_id: 'old', previous_page_token: 'older', incomplete: true } })
    await receive({ thread: chat, end_read_id: 'old' })
    return { done: new Promise(resolve => { finish = resolve }) }
  } } }
  const owner = new ChatViewOwner(host, 'c'), pending = owner.earlier()
  await flush(); owner.live(); assert.equal(signal.aborted, true)
  finish(); await pending
  assert.equal(owner.snapshot().error, undefined)
  owner.dispose()
})

function barrier() {
  let release
  const wait = new Promise(resolve => { release = resolve })
  return { wait, release }
}
test('composer completion preserves edits during upload, including same-text context/image edits, after restart', async () => {
  const { ComposerDraft } = require(resolve(root, 'outbox.js'))
  for (const contextual of [false, true]) {
    const io = storage(), store = await HostStore.open(io, connection), outbox = new ClientOutbox(store)
    const composer = new ComposerDraft(outbox, 'chat-a')
    const attachments = contextual ? [{ label: 'A quote', quote: { text: 'A' } }] : []
    await composer.edit({ text: 'same text', attachments, images: [{ id: 'image-a', size: 1, sha256: 'AA==', mediaType: 'image/png' }] }, context)
    const a = composer.snapshot(), captured = barrier(), finish = barrier()
    const operation = (async () => {
      captured.release(); await finish.wait
      if (contextual) return composer.clear(a)
      const cleared = await outbox.enqueue({ ...entry(), context: a.context, draft: a.draft, images: a.images }, a.revision)
      return composer.clear(a, cleared)
    })()
    await captured.wait
    const contextB = { ...context, history_epoch: '8' }
    await composer.edit({ attachments: [{ label: 'B quote', quote: { text: 'B' } }], images: [{ id: 'image-b', size: 2, sha256: 'AQ==', mediaType: 'image/png' }] }, contextB)
    const b = composer.snapshot()
    finish.release()
    assert.equal(await operation, false)
    assert.deepEqual(composer.snapshot(), b)
    const restored = new ComposerDraft(new ClientOutbox(await HostStore.open(io, connection)), 'chat-a')
    assert.deepEqual(restored.snapshot(), b)
    if (!contextual) {
      assert.equal(store.snapshot().outbox[0].images[0].id, 'image-a')
      assert.deepEqual(store.snapshot().outbox[0].context, context)
    }
  }
})
test('outbox commit racing a newer edit clears only submitted UI version and cannot resurrect on navigation', async () => {
  const { ComposerDraft } = require(resolve(root, 'outbox.js'))
  const io = storage(), store = await HostStore.open(io, connection), outbox = new ClientOutbox(store)
  const composer = new ComposerDraft(outbox, 'chat-a')
  await composer.edit({ text: 'A' }, context)
  const a = composer.snapshot(), entered = barrier(), finish = barrier(), write = io.writeState.bind(io)
  io.writeState = async (key, json) => { entered.release(); await finish.wait; return write(key, json) }
  const enqueue = outbox.enqueue({ ...entry(), draft: a.draft }, a.revision)
  await entered.wait
  const savingB = composer.edit({ text: 'B' })
  const b = composer.snapshot()
  finish.release()
  assert.equal(await composer.clear(a, await enqueue), false)
  await savingB
  assert.deepEqual(composer.snapshot(), b)
  assert.deepEqual(new ComposerDraft(new ClientOutbox(await HostStore.open(io, connection)), 'chat-a').snapshot(), b)
  io.writeState = write
  const cleared = await outbox.enqueue({ ...entry('command-b'), draft: b.draft }, b.revision)
  assert.equal(await composer.clear(b, cleared), true)
  assert.equal(composer.snapshot().draft.text, '')
  assert.equal((await HostStore.open(io, connection)).snapshot().drafts['chat-a'], undefined)
  await composer.edit({ text: 'retained on failure' }, context)
  io.fail = true
  await assert.rejects(composer.clear(composer.snapshot()), /disk full/)
  assert.equal(composer.snapshot().draft.text, 'retained on failure')
})

test('contextual Send recovery keeps original envelope while the next composer draft survives', async () => {
  const { ComposerDraft } = require(resolve(root, 'outbox.js'))
  const io = storage(), store = await HostStore.open(io, connection), outbox = new ClientOutbox(store)
  const composer = new ComposerDraft(outbox, 'chat-a')
  await composer.edit({ text: 'A', attachments: [{ label: 'quote', quote: { text: 'context A' } }] }, context)
  const a = composer.snapshot()
  const request = { command: { command_id: 'send-original', command_epoch: '2' }, target, message_id: 'message-original', draft: a.draft, expected_context: a.context }
  await store.update(state => { state.epoch = '2'; state.mutations = [{ method: 'ThreadService/SendMessage', commandId: request.command.command_id, epoch: '2', chatRequest: chatRequest('ThreadService/SendMessage', request) }] })
  const captured = barrier(), finish = barrier()
  let delivered = false, attempts = 0
  const transport = { async unary(method, body) {
    if (method === 'HostService/GetCommand') return delivered ? { state: 1, receipt: { host_id: 'host-a', ...request.command, state: 3 } } : { state: 2 }
    assert.equal(method, 'ThreadService/SendMessage'); assert.deepEqual(body, request)
    attempts++; captured.release(); await finish.wait; delivered = true
    throw Error('accepted but reply lost')
  } }
  const operation = assert.rejects(retryChatMutation(store, transport, 'send-original'), /reply lost/)
  await captured.wait
  await composer.edit({ text: 'B', attachments: [] }, { ...context, history_epoch: '8' })
  const b = composer.snapshot(); finish.release(); await operation
  const restored = await HostStore.open(io, connection)
  assert.equal((await retryChatMutation(restored, transport, 'send-original')).state, 3)
  assert.equal(attempts, 1)
  assert.deepEqual(new ComposerDraft(new ClientOutbox(restored), 'chat-a').snapshot(), b)
  assert.deepEqual(restoreChatRequest('ThreadService/SendMessage', restored.snapshot().mutations[0].chatRequest), request)
})
