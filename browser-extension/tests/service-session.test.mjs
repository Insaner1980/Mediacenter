import assert from 'node:assert/strict';
import { test } from 'node:test';

const url = 'https://www.youtube.com/';
const managed = { kind: 'managed', service: 'youtube' };

function event() {
  const listeners = [];
  return {
    addListener(listener) { listeners.push(listener); },
    emit(...args) { for (const listener of listeners) listener(...args); },
  };
}

async function fixture() {
  const { BrowserSession } = await import('../browser-session.mjs');
  const states = [];
  const input = [];
  const overlays = [];
  const windows = [];
  const tabs = new Map();
  const removed = [];
  const onUpdated = event();
  const onRemoved = event();
  const onDetach = event();
  const onEvent = event();
  let activeTab = 10;
  let width = 100;
  let inputError = false;
  let focused = true;
  let overlayEnable;
  let layoutFailures = 0;
  let inputSent;
  function addTab(id, url, openerTabId) {
    tabs.set(id, { id, windowId: 7, url, status: 'complete', openerTabId, loader: 1 });
  }
  const chrome = {
    runtime: { getURL: name => 'chrome-extension://fixture/' + name },
    tabs: {
      onUpdated, onRemoved, onReplaced: event(), onActivated: event(),
      async query() { return [{ ...tabs.get(activeTab) }]; },
      async get(id) { return { ...tabs.get(id), active: id === activeTab }; },
      async update(id, options) {
        if (options.url) {
          const tab = tabs.get(id);
          Object.assign(tab, { pendingUrl: options.url, status: 'loading' });
          onUpdated.emit(id, { status: 'loading' }, { ...tab });
        }
        return { ...tabs.get(id) };
      },
      async remove(id) { removed.push(id); tabs.delete(id); onRemoved.emit(id); },
    },
    windows: {
      onFocusChanged: event(), onBoundsChanged: event(),
      async get(id) { return { id, focused }; },
      async create(options) {
        windows.push(options);
        addTab(10, options.url ?? 'about:blank');
        return { id: 7, tabs: [{ ...tabs.get(10) }] };
      },
      async update() {},
    },
    debugger: {
      onDetach, onEvent,
      async attach() {},
      async detach() {},
      async sendCommand({ tabId }, method, params) {
        const tab = tabs.get(tabId);
        if (method === 'Page.getFrameTree') {
          return { frameTree: { frame: { id: 'root', loaderId: String(tab.loader), url: tab.url } } };
        }
        if (method === 'Page.getLayoutMetrics') {
          if (layoutFailures > 0) {
            layoutFailures -= 1;
            throw Error('Layout metrics temporarily unavailable');
          }
          return { cssLayoutViewport: { clientWidth: width, clientHeight: 80 } };
        }
        if (method.startsWith('Overlay.')) {
          overlays.push(method);
          if (method === 'Overlay.enable') await overlayEnable?.();
        }
        if (method.startsWith('Input.')) {
          input.push([tabId, method, params]);
          await inputSent?.(params);
          if (inputError) throw Error('private CDP error');
        }
      },
    },
  };
  const bridge = new BrowserSession(chrome, state => states.push(state));
  const launch = (fields = {}, profile = managed) => bridge.command({
    type: 'launch', url, session: 'a718146d-94e1-4fe9-b2e8-64fa61adc6b2',
    nonce: 'a8cd0db0-6e24-4dc1-9366-8d4cf8a7ce80', ...fields,
  }, profile);
  const latest = () => states.at(-1);
  const command = (type, fields = {}, generation = latest().generation) => bridge.command({
    type, session: latest().session, generation, ...fields,
  }, managed);
  const send = (params, generation) => command('input', { input: params }, generation);
  async function navigate(destination) {
    const tab = tabs.get(10);
    delete tab.pendingUrl;
    Object.assign(tab, { status: 'complete', url: destination, loader: tab.loader + 1 });
    onUpdated.emit(10, { url: destination, status: 'complete' }, { ...tab });
    await new Promise(resolve => setImmediate(resolve));
  }
  return {
    addTab, launch, latest, navigate, command, send, input, overlays, removed, states, windows,
    setActive(id) { activeTab = id; },
    focusElsewhere() { focused = false; chrome.windows.onFocusChanged.emit(8); },
    waitToEnableOverlay(callback) { overlayEnable = callback; },
    failLayoutOnce() { layoutFailures += 1; },
    onInputSent(callback) { inputSent = callback; },
    adopt() { return bridge.adoptActivePopup(); },
    async closeTab(id) {
      tabs.delete(id);
      onRemoved.emit(id);
      await new Promise(resolve => setImmediate(resolve));
    },
    resize(nextWidth) { width = nextWidth; onEvent.emit({ tabId: 10 }, 'Page.frameResized', {}); },
    detach() { onDetach.emit({ tabId: 10 }); },
    failInput() { inputError = true; },
  };
}

test('a launch grant binds one new tab to an exact service and cannot be reused', async () => {
  const f = await fixture();
  await assert.rejects(f.launch({ url: url + '?redirect=private' }));
  await assert.rejects(f.launch({}, { kind: 'managed', service: 'netflix' }));
  await assert.rejects(f.launch({}, { kind: 'ordinary' }));
  assert.equal(f.windows.length, 0);
  await f.launch({}, { kind: 'managed', service: 'youtube' });
  assert.equal(f.windows.length, 1);
  assert.equal(f.latest().ready, false);
  await f.navigate(url);
  assert.equal(f.latest().ready, true);
  f.addTab(11, url, 10);
  await f.send({ type: 'key', key: 'Tab' });
  assert.deepEqual(f.input.map(([id]) => id), [10, 10]);
  await f.command('close');
  assert.deepEqual(f.removed, [10]);
  assert.equal(f.latest().closed, true);
  await assert.rejects(f.launch());
});

test('HBO Max launches in an explicitly enrolled ordinary Chrome profile', async () => {
  const f = await fixture();
  const hbo = 'https://www.hbomax.com/';
  await assert.rejects(f.launch({ url: hbo }, { kind: 'managed', service: 'hbo_max' }));
  await f.launch({ url: hbo }, { kind: 'ordinary' });
  await f.navigate('https://play.hbomax.com/');
  assert.equal(f.latest().ready, true);
});

test('an unexpected origin invalidates input and only current explicit approval enables it', async () => {
  const f = await fixture();
  await f.launch();
  await f.navigate(url);
  const generation = f.latest().generation;
  await f.navigate('https://login.example/form?session=private#token');
  assert.equal(f.latest().ready, false);
  assert.equal(f.latest().pendingOrigin, 'https://login.example');
  assert.ok(!JSON.stringify(f.states).includes('private'));
  await assert.rejects(f.command('approve', {}, generation), /stale_target/);
  await assert.rejects(f.send({ type: 'text', text: 'synthetic' }), /origin_pending/);
  assert.equal(f.input.length, 0);
  await f.command('approve');
  assert.equal(f.latest().ready, true);
  await f.send({ type: 'text', text: 'synthetic' });
  assert.equal(f.input[0][1], 'Input.insertText');
  f.detach();
  assert.equal(f.latest().closed, true);
  await assert.rejects(f.send({ type: 'text', text: 'again' }), /stale_target/);
});

test('resize invalidates coordinates, and queued high-rate input is coalesced and ordered', async () => {
  const f = await fixture();
  await f.launch();
  await f.navigate(url);
  const generation = f.latest().generation;
  await f.send({ type: 'pointer', x: 0.5, y: 1, inputSequence: '1' });
  const first = f.send({ type: 'pointer', x: 0.1, y: 0, inputSequence: '2' });
  const superseded = assert.rejects(first, /bridge_unavailable/);
  await f.send({ type: 'pointer', x: 0.25, y: 0.5, inputSequence: '3' });
  await superseded;
  await assert.rejects(f.send({ type: 'scroll', deltaX: 0, deltaY: 10, inputSequence: '2' }), /stale_target/);
  assert.deepEqual(f.input.map(([, , params]) => params.x), [49.5, 24.75]);
  f.resize(200);
  await new Promise(resolve => setImmediate(resolve));
  assert.notEqual(f.latest().generation, generation);
  await assert.rejects(f.send({ type: 'click', button: 'left', x: 1, y: 1 }, generation), /stale_target/);
  await f.send({ type: 'click', button: 'left', x: 1, y: 1 });
  assert.equal(f.input.at(-1)[2].x, 199);
});

test('a failed text delivery is attempted once and arbitrary methods are rejected', async () => {
  const f = await fixture();
  await f.launch();
  await f.navigate(url);
  f.failInput();
  await assert.rejects(f.send({ type: 'text', text: 'synthetic' }), /bridge_unavailable/);
  await assert.rejects(f.send({ type: 'Runtime.evaluate', expression: 'alert(1)' }));
  assert.equal(f.input.length, 1);
  assert.ok(!JSON.stringify(f.states).includes('synthetic'));
});

test('losing browser focus during overlay setup cancels the marker', async () => {
  const f = await fixture();
  await f.launch();
  await f.navigate(url);
  const enabling = Promise.withResolvers();
  const finishEnable = Promise.withResolvers();
  f.waitToEnableOverlay(() => {
    enabling.resolve();
    return finishEnable.promise;
  });
  const pointer = f.send({ type: 'pointer', x: 0.5, y: 1, inputSequence: '1' });
  await enabling.promise;
  f.focusElsewhere();
  finishEnable.resolve();
  await pointer;
  assert.equal(f.input.length, 1);
  assert.ok(!f.overlays.includes('Overlay.highlightRect'));
});

test('discrete input rejected behind a pointer reports that it was never dispatched', async () => {
  const f = await fixture();
  await f.launch();
  await f.navigate(url);
  const moving = Promise.withResolvers();
  const finishMove = Promise.withResolvers();
  f.onInputSent(({ type }) => {
    if (type === 'mouseMoved') {
      moving.resolve();
      return finishMove.promise;
    }
  });
  const pointer = f.send({ type: 'pointer', x: 0.5, y: 1, inputSequence: '1' });
  await moving.promise;
  try {
    await assert.rejects(f.send({ type: 'text', text: 'synthetic' }), /^Error: busy$/);
    assert.deepEqual(f.input.map(([, method]) => method), ['Input.dispatchMouseEvent']);
  } finally {
    finishMove.resolve();
    await pointer;
  }
});

test('a transient layout error after page completion leaves input disabled until a fresh snapshot succeeds', async () => {
  const f = await fixture();
  await f.launch();
  f.failLayoutOnce();
  await f.navigate(url);
  assert.equal(f.latest().ready, false);
  await assert.rejects(f.send({ type: 'text', text: 'wait for snapshot' }), /stale_target/);
  await new Promise(resolve => setTimeout(resolve, 150));
  assert.equal(f.latest().ready, true);
  await f.send({ type: 'text', text: 'snapshot available' });
  assert.equal(f.input.length, 1);
});

test('a viewport resize lets an already sent mouse press finish in the same document', async () => {
  const f = await fixture();
  await f.launch();
  await f.navigate(url);
  const pressed = Promise.withResolvers();
  const finishPress = Promise.withResolvers();
  f.onInputSent(({ type }) => {
    if (type === 'mousePressed') {
      pressed.resolve();
      return finishPress.promise;
    }
  });
  const generation = f.latest().generation;
  const click = f.send({ type: 'click', button: 'left', x: 0.5, y: 0.5 });
  await pressed.promise;
  f.resize(200);
  await new Promise(resolve => setImmediate(resolve));
  assert.notEqual(f.latest().generation, generation);
  finishPress.resolve();
  await click;
  assert.deepEqual(f.input.map(([, , params]) => params.type), ['mousePressed', 'mouseReleased']);
});

test('a navigation prevents mouse release in a replacement document', async () => {
  const f = await fixture();
  await f.launch();
  await f.navigate(url);
  const pressed = Promise.withResolvers();
  const finishPress = Promise.withResolvers();
  f.onInputSent(({ type }) => {
    if (type === 'mousePressed') {
      pressed.resolve();
      return finishPress.promise;
    }
  });
  const click = f.send({ type: 'click', button: 'left', x: 0.5, y: 0.5 });
  const result = assert.rejects(click, /stale_target/);
  await pressed.promise;
  await f.navigate(url + 'watch');
  finishPress.resolve();
  await result;
  assert.deepEqual(f.input.map(([, , params]) => params.type), ['mousePressed']);
});

test('only an explicit local action adopts a related popup, which requires fresh approval', async () => {
  const f = await fixture();
  await f.launch();
  await f.navigate(url);
  const originalGeneration = f.latest().generation;
  f.addTab(11, 'https://accounts.google.com/signin', 10);
  f.addTab(12, 'https://accounts.google.com/signin', 999);
  f.setActive(12);
  await assert.rejects(f.adopt());
  f.setActive(11);
  await f.send({ type: 'text', text: 'original' });
  assert.equal(f.input.at(-1)[0], 10);
  await f.adopt();
  assert.notEqual(f.latest().generation, originalGeneration);
  assert.equal(f.latest().pendingOrigin, 'https://accounts.google.com');
  await assert.rejects(f.send({ type: 'text', text: 'blocked' }), /origin_pending/);
  await f.command('approve');
  await f.send({ type: 'text', text: 'popup' });
  assert.equal(f.input.at(-1)[0], 11);
  await f.closeTab(11);
  assert.equal(f.latest().ready, true);
  await f.send({ type: 'text', text: 'returned' });
  assert.equal(f.input.at(-1)[0], 10);
});
