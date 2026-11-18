import test from 'node:test';
import assert from 'node:assert/strict';
import { TargetRegistry } from '../target-registry.mjs';
test('only the active generation can dispatch to the enrolled tab', () => {
  const registry = new TargetRegistry();
  registry.enroll('profile');
  registry.issue('profile', 'nonce', 'http://127.0.0.1:41739', 100);
  registry.launch('profile', 'nonce', 1);
  registry.attach(9, 'session', 'generation-1');
  registry.attach(9, 'session', 'generation-2');
  assert.equal(registry.validate('session', 'generation-2', 9).tabId, 9);
  assert.throws(() => registry.validate('session', 'generation-1', 9), /stale_target/);
  assert.throws(() => registry.validate('session', 'generation-2', 10), /stale_target/);
  registry.invalidate();
  assert.throws(() => registry.validate('session', 'generation-2', 9), /stale_target/);
});
