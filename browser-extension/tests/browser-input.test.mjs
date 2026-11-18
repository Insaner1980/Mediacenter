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
    if (method === 'Page.getLayoutMetrics') return { cssLayoutViewport: { clientWidth: 800, clientHeight: 450 } };
  } };
  await assert.rejects(runInputProof(debuggerApi, 42, 'café 🦊', { x: 80, y: 20 }, () => {
    if (!current) throw Error('stale_target');
  }), /stale_target/);
  assert.deepEqual(commands.map(([method]) => method), ['Page.getLayoutMetrics', 'Input.insertText', 'Input.dispatchKeyEvent']);
  assert.equal(commands[1][1].text, 'café 🦊');
});

test('proof rejects changed viewport metrics before sending any input', async () => {
  const commands = [];
  const debuggerApi = { async sendCommand(_, method) {
    commands.push(method);
    return { cssLayoutViewport: { clientWidth: 70, clientHeight: 450 } };
  } };
  const approved = { width: 800, height: 450, generation: 'measured' };
  await assert.rejects(runInputProof(debuggerApi, 42, 'synthetic', { x: 80, y: 20 }, () => {}, approved), /stale_viewport/);
  assert.deepEqual(commands, ['Page.getLayoutMetrics']);
});
