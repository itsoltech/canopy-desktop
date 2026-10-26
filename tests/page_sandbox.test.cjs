const {test} = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const vm = require('node:vm');
const path = require('node:path');
const script = name => fs.readFileSync(path.join(__dirname, '../src/chat', name), 'utf8');

test('the native container follows the child height and accepts no foreign messages', async () => {
  const frame = {style: {}, contentWindow: {}};
  let message;
  const window = {addEventListener: (type, callback) => {
    assert.equal(type, 'message');
    message = callback;
  }};
  vm.runInNewContext(script('page_host.js'), {
    window, document: {querySelector: () => frame},
  });
  const send = (canopyPage, value, source = frame.contentWindow) =>
    message({source, data: {canopyPage, value}});
  send('height', 100, {});
  assert.equal(frame.style.height, undefined);
  send('height', 123.4);
  assert.equal(frame.style.height, '124px');
  send('height', 48);
  assert.equal(frame.style.height, '48px');
  for (const invalid of [NaN, Infinity, -1, '999px']) send('height', invalid);
  assert.equal(frame.style.height, '48px');
  let ready = false;
  window.__canopyReady.then(() => { ready = true; });
  send('ready', true, {});
  await Promise.resolve();
  assert.equal(ready, false);
  send('ready', true);
  await window.__canopyReady;
  assert.equal(ready, true);
});

test('content resize and fonts settle before the remote snapshot becomes ready', async () => {
  let height = 60;
  let resize;
  let load;
  let finishFonts;
  const fonts = new Promise(resolve => { finishFonts = resolve; });
  const sent = [];
  vm.runInNewContext(script('page_frame.js'), {
    parent: {postMessage: data => sent.push(data)},
    window: {addEventListener: (name, callback) => { if (name === 'load') load = callback; }},
    document: {
      readyState: 'complete', documentElement: {},
      body: {getBoundingClientRect: () => ({height})},
      fonts: {ready: fonts}, getAnimations: () => [],
    },
    getComputedStyle: () => ({marginTop: '4px', marginBottom: '6px'}),
    ResizeObserver: class {
      constructor(callback) { resize = callback; }
      observe() {}
    },
    setTimeout: () => { /* Font/animation deadlines do not fire in this test. */ },
  });
  assert.equal(sent.at(-1).value, 70);
  resize();
  assert.equal(sent.length, 1);
  height = 150;
  resize();
  assert.equal(sent.at(-1).value, 160);
  const settling = load();
  assert.ok(!sent.some(event => event.canopyPage === 'ready'));
  height = 30;
  finishFonts();
  await settling;
  assert.equal(sent.at(-2).value, 40);
  assert.equal(sent.at(-1).canopyPage, 'ready');
});
