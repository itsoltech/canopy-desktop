const { test } = require('node:test')
const assert = require('node:assert/strict')
const { resolve } = require('node:path')
const root = process.env.CANOPY_TEST_BUILD
if (!root) throw Error('Run using scripts/test-remote.mjs')
const { CatalogFeed, readCatalog } = require(resolve(root, 'catalog.js'))

const boundary = sequence => ({ runtime_epoch: 'e', stream_id: 'catalog/1', sequence: String(sequence) })
const chat = id => ({ chat: { host_id: 'h', chat_id: id }, title: id })
const page = (projects, chats) => ({ host_id: 'h', projects, chats, terminals: [] })
const part = (sequence, first, last, value) => ({ cursor: boundary(sequence), snapshot_part: { boundary: boundary(sequence), page: value, first, last } })
const project = checkouts => ({ project_id: '/repo', name: 'repo', path: '/repo', checkouts: checkouts.map(path => ({ name: path, path })) })

test('a paged catalog is installed whole, a split project merged, deltas after its boundary', () => {
  const feed = new CatalogFeed('h')
  const old = page([], [chat('old')])
  assert.equal(feed.apply(old, part(3, true, false, page([project(['/a'])], []))), undefined)
  assert.equal(feed.apply(old, part(3, false, false, page([project(['/b'])], [chat('x')]))), undefined)
  const whole = feed.apply(old, part(3, false, true, page([], [chat('y')])))
  assert.deepEqual(whole.projects.map(p => p.checkouts.map(c => c.path)), [['/a', '/b']])
  assert.deepEqual(whole.chats.map(c => c.title), ['x', 'y'])
  assert.deepEqual(old.chats.map(c => c.title), ['old'], 'the installed catalog is never changed in place')
  const next = feed.apply(whole, { cursor: boundary(4), chat: chat('z') })
  assert.deepEqual(next.chats.map(c => c.title), ['x', 'y', 'z'])
  assert.throws(() => feed.apply(next, { cursor: boundary(6), chat: chat('q') }), /gap/)
})

test('a delta or a foreign boundary during a snapshot drops the staging', () => {
  const feed = new CatalogFeed('h')
  const old = page([], [chat('old')])
  feed.apply(old, part(1, true, false, page([], [chat('a')])))
  assert.throws(() => feed.apply(old, { cursor: boundary(2), chat: chat('b') }), /during a snapshot/)
  // The dropped snapshot cannot be completed by its last page.
  assert.throws(() => feed.apply(old, part(1, false, true, page([], [chat('c')]))), /before the first page/)
  feed.apply(old, part(5, true, false, page([], [chat('a')])))
  assert.throws(() => feed.apply(old, part(6, false, true, page([], [chat('c')]))), /boundary changed/)
})

test('readCatalog returns only a complete catalog', async () => {
  const stream = events => ({ async subscribe(method, request, receive) {
    assert.equal(method, 'CatalogService/ReadCatalog'); assert.equal(request.host_id, 'h')
    return { done: (async () => { for (const event of events) await receive(event) })() }
  } })
  const catalog = await readCatalog(stream([part(0, true, false, page([], [chat('a')])), part(0, false, true, page([], [chat('b')]))]), 'h')
  assert.deepEqual(catalog.chats.map(c => c.title), ['a', 'b'])
  await assert.rejects(readCatalog(stream([part(0, true, false, page([], [chat('a')]))]), 'h'), /ended before/)
})
