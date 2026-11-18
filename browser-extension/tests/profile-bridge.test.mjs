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
  const onAlarm = event();
  const chrome = {
    runtime: {
      onStartup: event(), onInstalled: event(),
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
        async get(key) { return { [key]: storage[key] }; },
        async set(fields) { Object.assign(storage, fields); },
        async remove(key) { delete storage[key]; },
      },
    },
    alarms: { onAlarm, async get() {}, async create() {}, async clear() {} },
    tabs: { onRemoved: event(), onUpdated: event(), async remove(id) { removed.push(id); } },
  };
  let disconnected = 0;
  const session = { async command() {}, disconnect() { disconnected += 1; } };
  return {
    ports, removed,
    bridge: () => new ProfileBridge(chrome, session),
    disconnected: () => disconnected,
  };
}

test('production enrollment is explicit, survives a worker restart and retires on port loss', async () => {
  const { bridge, ports, removed, disconnected } = await fixture();
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
  await initial.wake(nonce, 17);
  assert.deepEqual(first.messages.at(-1), { type: 'wake', nonce });
  const reply = command => first.onMessage.emit({ id: crypto.randomUUID(), command });
  reply({ type: 'launch', nonce: crypto.randomUUID() });
  await Promise.resolve();
  assert.deepEqual(removed, []);
  reply({ type: 'launch', nonce });
  await Promise.resolve();
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
  const { bridge, ports, removed } = await fixture();
  const profile = bridge();
  await profile.enroll('managed', 'youtube');
  const [port] = ports;
  port.onMessage.emit({ type: 'welcome' });
  const first = crypto.randomUUID();
  const second = crypto.randomUUID();
  await profile.wake(first, 17);
  await profile.wake(second, 18);
  for (const nonce of [first, second]) {
    port.onMessage.emit({ id: crypto.randomUUID(), command: { type: 'launch', nonce } });
    await Promise.resolve();
  }
  assert.deepEqual(removed, [17, 18]);
});

test('a failed native wake write is reported to the enrolled profile', async () => {
  const { bridge, ports } = await fixture();
  const profile = bridge();
  const nonce = crypto.randomUUID();
  await profile.enroll('managed', 'youtube');
  const [port] = ports;
  port.onMessage.emit({ type: 'welcome' });
  port.postMessage = () => { throw Error('native port disconnected'); };
  await assert.rejects(profile.wake(nonce, 17), /bridge_unavailable/);
  assert.equal((await profile.status()).enrolled, true);
  assert.equal((await profile.status()).connected, false);
});

test('wake identifies a profile with no enrollment', async () => {
  const { bridge } = await fixture();
  await assert.rejects(bridge().wake(crypto.randomUUID(), 17), /not_enrolled/);
});
