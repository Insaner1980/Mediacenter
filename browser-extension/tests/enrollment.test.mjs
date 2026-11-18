import test from 'node:test';
import assert from 'node:assert/strict';
import { TargetRegistry } from '../target-registry.mjs';
test('enrollment requires explicit profile and a fresh single-use launch nonce', () => {
 const registry = new TargetRegistry();
 assert.throws(() => registry.launch('p', 'n', 1));
 registry.enroll('p'); registry.issue('p','n','http://127.0.0.1:41739',100);
 assert.throws(() => registry.launch('other','n',1));
 assert.equal(registry.launch('p','n',1).origin,'http://127.0.0.1:41739');
 assert.throws(() => registry.launch('p','n',1));
 registry.issue('p','expired','http://127.0.0.1:41739',100);
 assert.throws(() => registry.launch('p','expired',100));
});
