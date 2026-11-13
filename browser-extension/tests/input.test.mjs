import assert from 'node:assert/strict';
import { test } from 'node:test';
import { dispatchBrowserInput, readViewport, requireSameViewport } from '../input.mjs';

const viewport = { width: 800, height: 450, generation: '4' };

function fixture() {
  const sent = [];
  const debuggerApi = { async sendCommand(target, method, params) { sent.push({ target, method, params }); } };
  const send = (request, validate = () => {}) => dispatchBrowserInput(debuggerApi, 23, request, viewport, validate);
  return { sent, send };
}

test('projects normalized coordinates into the current viewport and rejects a stale resize generation', async () => {
  const { sent, send } = fixture();
  await assert.rejects(send({ type: 'pointer', generation: '3', x: 1, y: 0.5 }), /stale_viewport/);
  assert.deepEqual(sent, []);
  await send({ type: 'pointer', generation: '4', x: 1, y: 0.5 });
  assert.deepEqual(sent, [{ target: { tabId: 23 }, method: 'Input.dispatchMouseEvent', params: { type: 'mouseMoved', x: 799, y: 224.5 } }]);
});

test('uses fixed browser keys and commits Unicode text once without accepting command or tab overrides', async () => {
  const { sent, send } = fixture();
  await assert.rejects(send({ type: 'key', key: 'Ctrl+A' }), /invalid_browser_key/);
  await assert.rejects(send({ type: 'text', text: 'café', tabId: 3 }), /invalid_browser_input/);
  await assert.rejects(send({ type: 'Runtime.evaluate', expression: 'alert(1)' }), /invalid_browser_input/);
  await assert.rejects(send({ type: 'text', text: '🦊'.repeat(4097) }), /invalid_browser_input/);
  assert.deepEqual(sent, []);
  await send({ type: 'text', text: 'café 🦊' });
  await send({ type: 'key', key: 'ShiftTab' });
  assert.deepEqual(sent.map(command => command.method), ['Input.insertText', 'Input.dispatchKeyEvent', 'Input.dispatchKeyEvent']);
  assert.ok(sent.every(command => command.target.tabId === 23));
  assert.equal(sent[0].params.text, 'café 🦊');
  assert.deepEqual(sent.slice(1).map(command => [command.params.type, command.params.key, command.params.modifiers]),
    [['keyDown', 'Tab', 8], ['keyUp', 'Tab', 8]]);
});

test('checks the target between press and release and rejects invalid coordinates', async () => {
  const { sent, send } = fixture();
  await assert.rejects(send({ type: 'click', generation: '4', x: NaN, y: 0.25, button: 'left' }), /invalid_browser_input/);
  await assert.rejects(send({ type: 'click', generation: '4', x: 0.5, y: 0.25, button: 'left' }, () => {
    if (sent.length) throw Error('stale_target');
  }), /stale_target/);
  assert.equal(sent.length, 1);
  assert.equal(sent[0].params.type, 'mousePressed');
});

test('queries CSS viewport metrics from the enrolled tab and refuses unusable dimensions', async () => {
  const sent = [];
  const debuggerApi = { async sendCommand(target, method) {
    sent.push([target, method]);
    return { cssLayoutViewport: { clientWidth: 800, clientHeight: 450 } };
  } };
  assert.deepEqual(await readViewport(debuggerApi, 23, '4', () => {}), viewport);
  assert.deepEqual(sent, [[{ tabId: 23 }, 'Page.getLayoutMetrics']]);
  await assert.rejects(readViewport({ async sendCommand() { return {}; } }, 23, '4', () => {}), /invalid_viewport/);
  let checks = 0;
  await assert.rejects(readViewport(debuggerApi, 23, '4', () => {
    if (++checks > 1) throw Error('stale_target');
  }), /stale_target/);
});

test('sends bounded scroll at a measured point and rejects stale or invalid scroll', async () => {
  const { sent, send } = fixture();
  const scroll = { type: 'scroll', generation: '4', x: 0.5, y: 0.5, deltaX: 0, deltaY: -120 };
  await assert.rejects(send({ ...scroll, deltaY: 4097 }), /invalid_browser_input/);
  await assert.rejects(send({ ...scroll, generation: '3' }), /stale_viewport/);
  await send(scroll);
  assert.deepEqual(sent, [{ target: { tabId: 23 }, method: 'Input.dispatchMouseEvent',
    params: { type: 'mouseWheel', x: 399.5, y: 224.5, deltaX: 0, deltaY: -120 } }]);
});

test('does not retry committed text when Chrome rejects or loses its response', async () => {
  let calls = 0;
  const debuggerApi = { async sendCommand() {
    calls++;
    throw Error('target disconnected');
  } };
  await assert.rejects(dispatchBrowserInput(debuggerApi, 23, { type: 'text', text: 'synthetic' }, null, () => {}), /disconnected/);
  assert.equal(calls, 1);
});

test('blocks queued input when the CSS viewport changes after coordinates were measured', async () => {
  let width = 800;
  const debuggerApi = { async sendCommand() {
    return { cssLayoutViewport: { clientWidth: width, clientHeight: 450 } };
  } };
  const previous = await readViewport(debuggerApi, 23, '4', () => {});
  await requireSameViewport(debuggerApi, 23, previous, () => {});
  width = 640;
  await assert.rejects(requireSameViewport(debuggerApi, 23, previous, () => {}), /stale_viewport/);
});
