// Actual Chrome with a manually installed extension, a private broker and synthetic tabs.
import assert from 'node:assert/strict';
import { execFileSync, spawn } from 'node:child_process';
import { randomUUID } from 'node:crypto';
import { once } from 'node:events';
import { mkdir, mkdtemp, readFile, rm, writeFile } from 'node:fs/promises';
import { createServer } from 'node:http';
import path from 'node:path';
import { setTimeout as delay } from 'node:timers/promises';

const root = path.resolve('.tools/browser-feasibility');
const id = (await readFile('browser-extension/extension-id.txt', 'utf8')).trim();
const hostName = 'io.emma.mediacenter.browser_proof';
const host = path.resolve('src-tauri/target/debug/mediacenter-browser-host');
const broker = path.resolve('src-tauri/target/debug/examples/browser-broker-proof');
const args = [`--user-data-dir=${root}/profile`, '--no-first-run', '--no-default-browser-check', '--remote-debugging-pipe'];
const mode = process.argv[2] ?? '--background-tab';
assert.ok(['--active-catalog', '--background-tab', '--external-focus'].includes(mode));
if (mode === '--external-focus') {
  assert.equal(process.env.MEDIACENTER_PRIVATE_DISPLAY, process.env.DISPLAY, 'Use a private Xvfb display');
  assert.ok(/^:[1-9][0-9]{2}$/.test(process.env.DISPLAY ?? ''), 'Require a dedicated Xvfb display');
  args.push('--disable-gpu');
}
const directory = await mkdtemp('/tmp/mediacenter-browser-proof-');
const env = { ...process.env, XDG_CONFIG_HOME: `${root}/config`, XDG_RUNTIME_DIR: directory };
const result = { extensionId: id, mode, passed: false };
const externalTitle = 'Synthetic Mediacenter focus fixture';
const syntheticText = 'synthetic café 🦊';
const page = `<!doctype html><meta charset="utf-8"><title>Synthetic browser proof</title>
<style>input,button{position:absolute;left:16px;width:160px;height:28px} input{top:10px} button{top:45px}</style>
<input aria-label="Synthetic input" autofocus><button type="button">Synthetic click</button>
<script>
  const input = document.querySelector('input');
  document.body.dataset.keys = document.body.dataset.clicks = '0';
  input.onkeydown = e => { if(e.key === 'Enter') document.body.dataset.keys++; };
  document.querySelector('button').onclick = () => document.body.dataset.clicks++;
</script>`;
let catalogRequests = 0;
const server = createServer((request, response) => {
  if (request.url === '/catalog') catalogRequests++;
  response.writeHead(200, { 'Content-Type': 'text/html; charset=utf-8' });
  response.end(page);
});
server.listen(0, '127.0.0.1');
await once(server, 'listening');
const port = server.address().port;
if (process.env.MEDIACENTER_BROWSER_APP_WINDOW === '1') args.push(`--app=http://127.0.0.1:${port}/app`);
const catalog = `http://127.0.0.1:${port}/catalog`;
const unrelated = `http://127.0.0.1:${port}/unrelated`;
const hostDir = path.join(root, 'profile/NativeMessagingHosts');
const manifest = path.join(hostDir, `${hostName}.json`);
let chrome;
let brokerProcess;
let external;
const externalText = path.join(directory, 'external-entry.txt');
let createdManifest = false;
let cdp;
let log = '';

function connect(child) {
  let sequence = 0;
  let buffer = '';
  const pending = new Map();
  const input = child.stdio[3];
  input.on('error', () => {});
  child.stdio[4].setEncoding('utf8').on('data', chunk => {
    buffer += chunk;
    for (let end; (end = buffer.indexOf('\0')) >= 0;) {
      const message = JSON.parse(buffer.slice(0, end));
      buffer = buffer.slice(end + 1);
      const finish = pending.get(message.id);
      if (finish) finish(message);
    }
  });
  return (method, params = {}, sessionId) => new Promise((resolve, reject) => {
    const id = ++sequence;
    const timer = setTimeout(() => {
      pending.delete(id);
      reject(Error(`timeout ${method}`));
    }, 10_000);
    pending.set(id, message => {
      pending.delete(id);
      clearTimeout(timer);
      if (message.error) reject(Error(`${method}: ${message.error.message}`));
      else resolve(message.result);
    });
    input.write(`${JSON.stringify({ id, method, params, ...(sessionId ? { sessionId } : {}) })}\0`);
  });
}

async function waitFor(check, description = 'synthetic browser state') {
  for (let attempt = 0; attempt < 35; attempt++) {
    const answer = await check();
    if (answer) return answer;
    await delay(150);
  }
  throw Error(`Timed out waiting for ${description}`);
}

async function session(targetId) {
  return (await cdp('Target.attachToTarget', { targetId, flatten: true })).sessionId;
}

async function evalPage(sessionId, expression) {
  const response = await cdp('Runtime.evaluate', { expression, returnByValue: true, awaitPromise: true }, sessionId);
  if (response.exceptionDetails) throw Error('browser fixture evaluation failed');
  return response.result.value;
}

function send(sessionId, request) {
  return evalPage(sessionId, `chrome.runtime.sendMessage(${JSON.stringify(request)})`);
}

async function state(sessionId) {
  return waitFor(() => evalPage(sessionId, `document.body?.dataset.keys ? {
    path: location.pathname, text: document.querySelector('input').value, keys: Number(document.body.dataset.keys),
    clicks: Number(document.body.dataset.clicks), focused: document.hasFocus()
  } : null`));
}

async function requestTarget(extension, profile) {
  const nonce = randomUUID();
  brokerProcess = spawn(broker, [profile, nonce, String(port)], { env, stdio: ['ignore', 'pipe', 'pipe'] });
  const brokerExit = once(brokerProcess, 'exit');
  const [ready] = await Promise.race([
    once(brokerProcess.stdout, 'data'),
    brokerExit.then(([code]) => { throw Error(`browser broker exited before ready (${code})`); }),
  ]);
  assert.equal(ready.toString().trim(), 'ready');
  const target = await send(extension, { type: 'prepareProof', nonce });
  const redeemed = await Promise.race([brokerExit, delay(1_000).then(() => null)]);
  assert.ok(redeemed?.[0] === 0 && Number.isInteger(target?.tabId),
    `Chrome launch fixture failed (broker ${redeemed?.[0] ?? 'no exit'}, ${target?.error ?? 'no reply'})`);
  return target;
}

try {
  await mkdir(hostDir, { recursive: true, mode: 0o700 });
  await writeFile(manifest, JSON.stringify({ name: hostName, description: 'Mediacenter disposable browser proof', path: host,
    type: 'stdio', allowed_origins: [`chrome-extension://${id}/`] }), { flag: 'wx', mode: 0o600 });
  createdManifest = true;
  const chromeEnv = mode === '--external-focus' ? { ...env, DBUS_SESSION_BUS_ADDRESS: `unix:path=${directory}/no-bus` } : env;
  chrome = spawn('/opt/google/chrome/google-chrome', args, { env: chromeEnv, stdio: ['ignore', 'ignore', 'pipe', 'pipe', 'pipe'] });
  chrome.stderr.setEncoding('utf8').on('data', chunk => { log += chunk; });
  cdp = connect(chrome);
  result.chrome = (await cdp('Browser.getVersion')).product;
  if (process.env.MEDIACENTER_BROWSER_APP_WINDOW === '1') {
    const app = await waitFor(async () => (await cdp('Target.getTargets')).targetInfos.find(info => info.url === `http://127.0.0.1:${port}/app`), 'managed app window');
    const appState = await state(await session(app.targetId));
    assert.equal(appState.path, '/app');
    result.managedAppWindowOpened = true;
  }
  const openExtension = async () => session((await cdp('Target.createTarget', { url: `chrome-extension://${id}/enroll.html` })).targetId);
  let extension = await openExtension();
  const readProfile = () => waitFor(() => evalPage(extension, 'globalThis.chrome?.storage?.local.get("profile").then(s => s.profile)'));
  const profile = await readProfile();
  if ((await send(extension, { type: 'probe' }))?.proofRevision !== 3) {
    await evalPage(extension, 'setTimeout(() => chrome.runtime.reload(), 0)');
    await delay(400);
    extension = await openExtension();
    assert.equal(await readProfile(), profile);
  }
  assert.equal((await send(extension, { type: 'probe' }))?.proofRevision, 3, 'Current extension worker was loaded');
  if (mode === '--external-focus') {
    const pingTarget = (await cdp('Target.createTarget', { url: unrelated })).targetId;
    const ping = await session(pingTarget);
    try {
      await state(ping);
    } catch (error) {
      result.loopbackProbe = await evalPage(ping, `({ url: location.protocol, title: document.title, chromeError: document.querySelector('#error-code')?.innerText })`);
      throw error;
    }
    await cdp('Target.closeTarget', { targetId: pingTarget });
  }
  const target = await requestTarget(extension, profile);
  result.nativeHostInvoked = true;
  const otherTarget = (await cdp('Target.createTarget', { url: unrelated })).targetId;
  const other = await session(otherTarget);
  await cdp('Target.activateTarget', { targetId: otherTarget });
  const catalogTarget = await waitFor(async () => (await cdp('Target.getTargets')).targetInfos.find(info => info.url === catalog));
  const catalogSession = await session(catalogTarget.targetId);
  const before = await state(other);
  const catalogBefore = await state(catalogSession);
  assert.deepEqual(catalogBefore, { path: '/catalog', text: '', keys: 0, clicks: 0, focused: false });
  assert.equal(before.path, '/unrelated');
  assert.deepEqual(await send(extension, { ...target, generation: randomUUID(), type: 'inputProof' }), { error: 'stale_target' });
  if (mode === '--active-catalog' || mode === '--external-focus') {
    await cdp('Target.activateTarget', { targetId: catalogTarget.targetId });
    if (mode === '--external-focus') await waitFor(() => evalPage(catalogSession, 'document.hasFocus()'), 'catalog focus before external app');
  }
  const externalFocused = () => {
    const active = execFileSync('xprop', ['-root', '_NET_ACTIVE_WINDOW'], { encoding: 'utf8' });
    const window = active.match(/0x[0-9a-f]+/i)?.[0];
    return window && window !== '0x0' && execFileSync('xprop', ['-id', window, 'WM_NAME'], { encoding: 'utf8' }).includes(externalTitle);
  };
  if (mode === '--external-focus') {
    await delay(450);
    const desktop = spawn('xwd', ['-root', '-silent', '-out', path.join(root, 'native-input-warning.xwd')], { stdio: 'ignore' });
    const [captureCode] = await once(desktop, 'exit');
    assert.equal(captureCode, 0);
    external = spawn('/usr/bin/python3', ['browser-extension/harness/focus-fixture.py', externalText], { stdio: 'ignore' });
    await waitFor(externalFocused, 'external app focus');
    result.externalWindowFocusedBefore = true;
  }
  assert.deepEqual(await send(extension, { ...target, type: 'inputProof' }), { submitted: true });
  await delay(500);
  const after = await state(catalogSession);
  const otherAfter = await state(other);
  if (mode === '--external-focus') {
    result.externalWindowFocusedAfter = Boolean(externalFocused());
    result.externalEntryStillOpen = external.exitCode === null;
    result.externalEntryUnchanged = (await readFile(externalText, 'utf8')) === '';
    result.externalEntryReceivedEnter = await readFile(path.join(directory, 'external-entry.enter')).then(() => true, error => {
      if (error.code === 'ENOENT') return false;
      throw error;
    });
  }
  result.focusBefore = { catalog: catalogBefore.focused, unrelated: before.focused };
  result.focusAfter = { catalog: after.focused, unrelated: otherAfter.focused };
  result.catalogInput = { correctText: after.text === syntheticText, keys: after.keys, clicks: after.clicks };
  result.unrelatedInput = { emptyText: !otherAfter.text, keys: otherAfter.keys, clicks: otherAfter.clicks };
  assert.deepEqual({ text: after.text, keys: after.keys, clicks: after.clicks },
    { text: syntheticText, keys: 1, clicks: 1 }, 'Catalog input');
  assert.deepEqual({ text: otherAfter.text, keys: otherAfter.keys, clicks: otherAfter.clicks },
    { text: before.text, keys: before.keys, clicks: before.clicks }, 'Unrelated input');
  result.tabScopedTextKeyAndClick = true;
  result.unrelatedTabUnchanged = true;
  if (result.managedAppWindowOpened) {
    const app = (await cdp('Target.getTargets')).targetInfos.find(info => info.url === `http://127.0.0.1:${port}/app`);
    const appWindow = await cdp('Browser.getWindowForTarget', { targetId: app.targetId });
    const catalogWindow = await cdp('Browser.getWindowForTarget', { targetId: catalogTarget.targetId });
    result.catalogInOriginalAppWindow = appWindow.windowId === catalogWindow.windowId;
    const appState = await state(await session(app.targetId));
    result.originalAppWindowUnchanged = !appState.text && appState.keys === 0 && appState.clicks === 0;
    assert.ok(result.originalAppWindowUnchanged);
  }
  await cdp('Page.navigate', { url: `${catalog}/different` }, catalogSession);
  await waitFor(() => evalPage(catalogSession, `location.pathname === '/catalog/different' && document.readyState === 'complete'`));
  assert.deepEqual(await send(extension, { ...target, type: 'inputProof' }), { error: 'stale_target' });
  result.navigationInvalidated = true;
  const workerBefore = (await send(extension, { type: 'probe' })).worker;
  await evalPage(extension, 'setTimeout(() => chrome.runtime.reload(), 0)');
  await delay(400);
  extension = await openExtension();
  assert.equal(await readProfile(), profile);
  assert.notEqual((await send(extension, { type: 'probe' })).worker, workerBefore);
  assert.deepEqual(await send(extension, { ...target, type: 'inputProof' }), { error: 'stale_target' });
  result.workerRestartRetiredTarget = true;
  const reconnected = await requestTarget(extension, profile);
  assert.notEqual(reconnected.tabId, target.tabId);
  assert.notEqual(reconnected.generation, target.generation);
  assert.deepEqual(await send(extension, { ...target, type: 'inputProof' }), { error: 'stale_target' });
  result.nativeHostReconnectedAfterWorkerReload = true;
  if (mode === '--external-focus') {
    assert.ok(result.externalEntryStillOpen && result.externalEntryUnchanged && !result.externalEntryReceivedEnter,
      'External entry must receive no text or Enter');
  }
  result.passed = true;
} catch (error) {
  result.catalogRequests = catalogRequests;
  result.error = error.message;
  process.exitCode = 1;
} finally {
  if (external?.exitCode === null) external.kill();
  if (brokerProcess?.exitCode === null) brokerProcess.kill();
  if (chrome?.exitCode === null) {
    const exited = once(chrome, 'exit');
    await cdp('Browser.close').catch(() => chrome.kill());
    await Promise.race([exited, delay(3_000, undefined, { ref: false }).then(() => chrome.kill())]);
  }
  server.closeAllConnections();
  server.close();
  if (createdManifest) await rm(manifest);
  await rm(directory, { recursive: true, force: true });
  const artifact = process.env.MEDIACENTER_BROWSER_APP_WINDOW === '1' ? 'app-window' : mode.slice(2);
  await writeFile(path.join(root, `native-input-${artifact}-result.json`), `${JSON.stringify(result, null, 2)}\n`);
  await writeFile(path.join(root, 'native-input-stderr.log'), log, { mode: 0o600 });
  console.log(result);
}
