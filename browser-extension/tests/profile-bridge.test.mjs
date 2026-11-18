import assert from 'node:assert/strict';
import { test } from 'node:test';

function event() {
  const listeners = [];
  return {
    addListener(listener) { listeners.push(listener); },
    emit(...args) { for (const listener of listeners) listener(...args); },
  };
}

async function fixture() {
  const { ProfileBridge } = await import('../profile-bridge.mjs');
  const storage = { profile: 'task-17-proof' };
  const ports = [];
  const removed = [];
  const tabs = new Map();
  const onAlarm = event();
  let readEnrollment;
  let launch;
  const chrome = {
    runtime: {
      onStartup: event(), onInstalled: event(),
      getURL: file => `chrome-extension://fixture/${file}`,
      connectNative(name) {
        const messages = [];
        const onDisconnect = event();
        const port = {
          name, messages, onDisconnect, onMessage: event(),
          postMessage(message) { messages.push(message); },
          disconnect() { onDisconnect.emit(); },
        };
        ports.push(port);
        return port;
      },
    },
    storage: {
      local: {
        async get(key) {
          await readEnrollment?.();
          return { [key]: storage[key] };
        },
        async set(fields) { Object.assign(storage, fields); },
        async remove(key) { delete storage[key]; },
      },
    },
    alarms: { onAlarm, async get() {}, async create() {}, async clear() {} },
    tabs: {
      onRemoved: event(), onUpdated: event(), onReplaced: event(),
      async get(id) { return { ...tabs.get(id) }; },
      async remove(id) { removed.push(id); },
    },
  };
  let disconnected = 0;
  const session = { command: async () => { await launch?.(); }, disconnect() { disconnected += 1; } };
  return {
    ports, removed,
    bridge: () => new ProfileBridge(chrome, session),
    disconnected: () => disconnected,
    waitForEnrollment(callback) { readEnrollment = callback; },
    waitForLaunch(callback) { launch = callback; },
    wake(profile, nonce, id) {
      tabs.set(id, { id, url: `${chrome.runtime.getURL('launch.html')}#${nonce}` });
      return profile.wake(nonce, id);
    },
    navigate(id, url) {
      tabs.set(id, { id, url });
      chrome.tabs.onUpdated.emit(id, { url });
    },
    reload(id) {
      chrome.tabs.onUpdated.emit(id, { status: 'loading' });
    },
    close(id) {
      tabs.delete(id);
      chrome.tabs.onRemoved.emit(id);
    },
  };
}

test('production enrollment is explicit, survives a worker restart and retires on port loss', async () => {
  const { bridge, ports, removed, disconnected, wake } = await fixture();
  const initial = bridge();
  await initial.connect();
  assert.equal(ports.length, 0);
  await initial.enroll('managed', 'youtube');
  const first = ports[0];
  assert.equal(first.name, 'io.emma.mediacenter.browser');
  const hello = first.messages[0];
  assert.deepEqual(hello, { type: 'hello', kind: 'managed', service: 'youtube', profile: hello.profile });
  assert.match(hello.profile, /^[a-f0-9-]{36}$/);
  first.onMessage.emit({ type: 'welcome' });
  assert.equal((await initial.status()).connected, true);
  const nonce = crypto.randomUUID();
  await wake(initial, nonce, 17);
  assert.deepEqual(first.messages.at(-1), { type: 'wake', nonce });
  const reply = command => first.onMessage.emit({ id: crypto.randomUUID(), command });
  reply({ type: 'launch', nonce: crypto.randomUUID() });
  await new Promise(resolve => setImmediate(resolve));
  assert.deepEqual(removed, []);
  reply({ type: 'launch', nonce });
  await new Promise(resolve => setImmediate(resolve));
  assert.deepEqual(removed, [17]);
  first.disconnect();
  assert.equal((await initial.status()).connected, false);
  assert.equal(disconnected(), 1);
  const next = bridge();
  await next.connect();
  assert.deepEqual(ports[1].messages[0], hello);
  await next.unenroll();
  await next.connect();
  assert.equal(ports.length, 2);
  assert.equal((await next.status()).enrolled, false);
});

test('each pending wake closes only its own launch tab after completing', async () => {
  const { bridge, ports, removed, wake } = await fixture();
  const profile = bridge();
  await profile.enroll('managed', 'youtube');
  const [port] = ports;
  port.onMessage.emit({ type: 'welcome' });
  const first = crypto.randomUUID();
  const second = crypto.randomUUID();
  await wake(profile, first, 17);
  await wake(profile, second, 18);
  port.onMessage.emit({ id: crypto.randomUUID(), command: { type: 'launch', nonce: second } });
  await new Promise(resolve => setImmediate(resolve));
  assert.deepEqual(removed, [18]);
  port.onMessage.emit({ id: crypto.randomUUID(), command: { type: 'launch', nonce: first } });
  await new Promise(resolve => setImmediate(resolve));
  assert.deepEqual(removed, [18, 17]);
});

test('navigation during enrollment lookup cancels the wake before the native host receives it', async () => {
  const f = await fixture();
  const profile = f.bridge();
  await profile.enroll('managed', 'youtube');
  const [port] = f.ports;
  const reading = Promise.withResolvers();
  const finishReading = Promise.withResolvers();
  f.waitForEnrollment(() => {
    reading.resolve();
    return finishReading.promise;
  });
  const nonce = crypto.randomUUID();
  const wake = f.wake(profile, nonce, 17);
  await reading.promise;
  f.navigate(17, 'https://example.com/');
  finishReading.resolve();
  await assert.rejects(wake, /bridge_unavailable/);
  assert.ok(port.messages.every(message => message.type !== 'wake'));
});

test('a closed launch tab leaves its nonce reserved until the pending command completes', async () => {
  const f = await fixture();
  const profile = f.bridge();
  await profile.enroll('managed', 'youtube');
  const [port] = f.ports;
  port.onMessage.emit({ type: 'welcome' });
  const nonce = crypto.randomUUID();
  const launching = Promise.withResolvers();
  const finishLaunch = Promise.withResolvers();
  f.waitForLaunch(() => {
    launching.resolve();
    return finishLaunch.promise;
  });
  await f.wake(profile, nonce, 17);
  port.onMessage.emit({ id: crypto.randomUUID(), command: { type: 'launch', nonce } });
  await launching.promise;
  f.close(17);
  try {
    await assert.rejects(f.wake(profile, nonce, 18), /bridge_unavailable/);
  } finally {
    finishLaunch.resolve();
    await new Promise(resolve => setImmediate(resolve));
  }
  assert.deepEqual(f.removed, []);
  assert.equal(port.messages.filter(message => message.type === 'wake').length, 1);
});

test('a same-address reload invalidates the current launch page binding', async () => {
  const f = await fixture();
  const profile = f.bridge();
  await profile.enroll('managed', 'youtube');
  const [port] = f.ports;
  port.onMessage.emit({ type: 'welcome' });
  const nonce = crypto.randomUUID();
  await f.wake(profile, nonce, 17);
  f.reload(17);
  port.onMessage.emit({ id: crypto.randomUUID(), command: { type: 'launch', nonce } });
  await new Promise(resolve => setImmediate(resolve));
  assert.deepEqual(f.removed, []);
  assert.equal(port.messages.at(-1).ok, true);
});

test('an expired launch command cannot close or clear a later attempt', async t => {
  t.mock.timers.enable({ apis: ['setTimeout'] });
  const f = await fixture();
  const profile = f.bridge();
  await profile.enroll('managed', 'youtube');
  const [port] = f.ports;
  t.after(() => port.disconnect());
  port.onMessage.emit({ type: 'welcome' });
  const nonce = crypto.randomUUID();
  const launching = Promise.withResolvers();
  const finishLaunch = Promise.withResolvers();
  f.waitForLaunch(() => {
    launching.resolve();
    return finishLaunch.promise;
  });
  await f.wake(profile, nonce, 17);
  port.onMessage.emit({ id: crypto.randomUUID(), command: { type: 'launch', nonce } });
  await launching.promise;
  t.mock.timers.tick(30_000);
  await f.wake(profile, nonce, 18);
  finishLaunch.resolve();
  await new Promise(resolve => setImmediate(resolve));
  assert.deepEqual(f.removed, []);
  await assert.rejects(f.wake(profile, nonce, 19), /bridge_unavailable/);
});

test('a failed native wake write is reported to the enrolled profile', async () => {
  const { bridge, ports, wake } = await fixture();
  const profile = bridge();
  const nonce = crypto.randomUUID();
  await profile.enroll('managed', 'youtube');
  const [port] = ports;
  port.onMessage.emit({ type: 'welcome' });
  port.postMessage = () => { throw Error('native port disconnected'); };
  await assert.rejects(wake(profile, nonce, 17), /bridge_unavailable/);
  assert.equal((await profile.status()).enrolled, true);
  assert.equal((await profile.status()).connected, false);
});

test('wake identifies a profile with no enrollment', async () => {
  const { bridge } = await fixture();
  await assert.rejects(bridge().wake(crypto.randomUUID(), 17), /not_enrolled/);
});
