import assert from 'node:assert/strict';
import { test } from 'node:test';
import { runInputProof } from '../input-proof.mjs';

test('proof binds fixed input to the enrolled tab and stops on generation loss', async () => {
  const commands = [];
  let current = true;
  const debuggerApi = { async sendCommand(target, method, params) {
    assert.deepEqual(target, { tabId: 42 });
    commands.push([method, params]);
    if (method === 'Input.dispatchKeyEvent') current = false;
  } };
  await assert.rejects(runInputProof(debuggerApi, 42, 'café 🦊', { x: 80, y: 20 }, () => {
    if (!current) throw Error('stale_target');
  }), /stale_target/);
  assert.deepEqual(commands.map(([method]) => method), ['Input.insertText', 'Input.dispatchKeyEvent']);
  assert.equal(commands[0][1].text, 'café 🦊');
});
