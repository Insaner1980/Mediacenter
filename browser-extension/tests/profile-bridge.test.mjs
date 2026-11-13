import assert from 'node:assert/strict';
import { test } from 'node:test';

function event() {
  const listeners = [];
  return {
    addListener(listener) { listeners.push(listener); },
    emit(...args) { for (const listener of listeners) listener(...args); },
  };
}

test('production enrollment is explicit, survives a worker restart and retires on port loss', async () => {
  const { ProfileBridge } = await import('../profile-bridge.mjs');
  const storage = { profile: 'task-17-proof' };
  const ports = [];
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
  };
  let disconnected = 0;
  const session = { disconnect() { disconnected += 1; } };
  const initial = new ProfileBridge(chrome, session);
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
  first.disconnect();
  assert.equal((await initial.status()).connected, false);
  assert.equal(disconnected, 1);
  const next = new ProfileBridge(chrome, session);
  await next.connect();
  assert.deepEqual(ports[1].messages[0], hello);
  await next.unenroll();
  await next.connect();
  assert.equal(ports.length, 2);
  assert.equal((await next.status()).enrolled, false);
});
