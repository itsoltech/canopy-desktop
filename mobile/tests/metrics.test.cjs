const { test } = require('node:test')
const assert = require('node:assert/strict')
const { join } = require('node:path')
const { timing, timingStart, diagnostics } = require(join(process.env.CANOPY_TEST_BUILD, 'metrics.js'))

test('local diagnostics count load without retaining content or unbounded samples', () => {
  const host = '18a3c472-3a42-42c7-9d0e-66662e2e6698', chat = '93147187-b9a7-4a9f-8595-945970e28eca'
  const before = diagnostics()
  timing(host, '/private/PROMPT_SECRET.txt', undefined, 'render', timingStart())
  timing(host, chat, 'TOKEN_SECRET', 'client_round_trip', timingStart())
  for (let i = 0; i < 2000; i++) timing(host, chat, undefined, 'client_receive_apply', timingStart())
  const after = diagnostics(), json = JSON.stringify(after)
  assert.equal(after.summaries.find(s => s.stage === 'client_receive_apply').count - before.summaries.find(s => s.stage === 'client_receive_apply').count, 2000)
  assert(after.records.length <= 256)
  assert(after.dropped_records > before.dropped_records)
  assert.equal(after.rejected_keys, before.rejected_keys + 2)
  assert(!json.includes('PROMPT_SECRET') && !json.includes('TOKEN_SECRET'))
  after.records.length = 0
  assert(diagnostics().records.length > 0, 'export cannot mutate retained diagnostics')
})
