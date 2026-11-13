// Exercise the production broker, Native Messaging host and MV3 worker on actual Chrome.
// The only provider page is fulfilled in-browser from a synthetic HTML fixture.
import assert from 'node:assert/strict';
import { spawn } from 'node:child_process';
import { once } from 'node:events';
import { cp, mkdir, mkdtemp, readFile, rm, writeFile } from 'node:fs/promises';
import path from 'node:path';
import { setTimeout as delay } from 'node:timers/promises';

const base = path.resolve('.tools/browser-feasibility');
const id = (await readFile('browser-extension/extension-id.txt', 'utf8')).trim();
const runtime = await mkdtemp('/tmp/mc-runtime-smoke-');
const profile = path.join(runtime, 'profile');
const env = { ...process.env, XDG_RUNTIME_DIR: runtime, XDG_CONFIG_HOME: path.join(runtime, 'config') };
const host = 'io.emma.mediacenter.browser';
const catalog = 'https://www.youtube.com/';
const binary = path.resolve('src-tauri/target/debug/mediacenter-browser-host');
const brokerBinary = path.resolve('src-tauri/target/debug/examples/browser-runtime-smoke');
let browser;
let broker;
let browserExit;
let brokerExit;
let call;
let chromeLog = '';
let diagnostics;
const result = { passed: false };

function connect(child) {
  const pending = new Map();
  const observed = new Set();
  let sequence = 0;
  let buffer = '';
  child.stdio[3].on('error', () => {});
  child.stdio[4].setEncoding('utf8').on('data', chunk => {
    buffer += chunk;
    for (let end; (end = buffer.indexOf('\0')) >= 0;) {
      const event = JSON.parse(buffer.slice(0, end));
      buffer = buffer.slice(end + 1);
      if (event.id) pending.get(event.id)?.(event);
      else for (const handler of observed) handler(event);
    }
  });
  return {
    call: (method, params = {}, sessionId) => new Promise((resolve, reject) => {
      const id = ++sequence;
      const timer = setTimeout(() => { pending.delete(id); reject(Error(`Timed out: ${method}`)); }, 5_000);
      pending.set(id, event => {
        clearTimeout(timer);
        pending.delete(id);
        if (event.error) reject(Error(`${method}: ${event.error.message}`));
        else resolve(event.result);
      });
      child.stdio[3].write(`${JSON.stringify({ id, method, params, sessionId })}\0`);
    }),
    observe: handler => observed.add(handler),
  };
}

async function waitFor(check, label, attempts = 150) {
  for (let attempt = 0; attempt < attempts; attempt++) {
    const result = await check();
    if (result) return result;
    await delay(80);
  }
  throw Error(`Timed out waiting for ${label}`);
}

async function evaluate(session, expression) {
  const value = await call('Runtime.evaluate', { expression, returnByValue: true, awaitPromise: true }, session);
  if (value.exceptionDetails) throw Error('Synthetic fixture evaluation failed');
  return value.result.value;
}

try {
  // Copy only after the manually installed disposable Chrome has exited.
  const source = path.join(base, 'profile');
  const profileLock = await import('node:fs/promises').then(fs => fs.lstat(path.join(source, 'SingletonLock')).catch(() => null));
  assert.ok(!profileLock, 'Close the disposable Chrome profile first');
  await cp(source, profile, { recursive: true });
  await mkdir(path.join(profile, 'NativeMessagingHosts'), { recursive: true });
  await writeFile(path.join(profile, 'NativeMessagingHosts', `${host}.json`), JSON.stringify({
    name: host, description: 'Disposable Mediacenter smoke', path: binary, type: 'stdio',
    allowed_origins: [`chrome-extension://${id}/`],
  }), { flag: 'wx', mode: 0o600 });
  broker = spawn(brokerBinary, [], { env, stdio: ['pipe', 'pipe', 'pipe'] });
  brokerExit = once(broker, 'exit');
  let output = '';
  let brokerError = '';
  broker.stdout.on('data', chunk => { output += chunk; });
  broker.stderr.on('data', chunk => { brokerError += chunk; });
  await waitFor(() => output.includes('listening'), 'broker socket');
  browser = spawn('/opt/google/chrome/google-chrome', [
    `--user-data-dir=${profile}`, '--no-first-run', '--no-default-browser-check', '--remote-debugging-pipe',
    '--no-proxy-server', '--host-resolver-rules=MAP * ~NOTFOUND', '--enable-logging=stderr',
  ], { env, stdio: ['ignore', 'ignore', 'pipe', 'pipe', 'pipe'] });
  browserExit = once(browser, 'exit');
  browser.stderr.on('data', chunk => { chromeLog = (chromeLog + chunk).slice(-16_000); });
  const connection = connect(browser);
  call = connection.call;
  result.chrome = (await call('Browser.getVersion')).product;
  let fixtureError;
  let catalogSession;
  connection.observe(event => {
    void (async () => {
      if (event.method === 'Target.attachedToTarget') {
        const { sessionId, targetInfo } = event.params;
        if (targetInfo.type === 'page') {
          await call('Fetch.enable', { patterns: [{ urlPattern: 'https://www.youtube.com/*' }] }, sessionId);
        }
        await call('Runtime.runIfWaitingForDebugger', {}, sessionId);
      } else if (event.method === 'Fetch.requestPaused') {
        const { requestId, request } = event.params;
        if (request.url === catalog) {
          catalogSession = event.sessionId;
          const body = Buffer.from('<!doctype html><title>Mediacenter production smoke</title><input autofocus aria-label="Synthetic input">').toString('base64');
          await call('Fetch.fulfillRequest', { requestId, responseCode: 200, responseHeaders: [{ name: 'Content-Type', value: 'text/html; charset=utf-8' }], body }, event.sessionId);
        } else {
          await call('Fetch.failRequest', { requestId, errorReason: 'BlockedByClient' }, event.sessionId);
        }
      }
    })().catch(error => { fixtureError = error; });
  });
  const popup = `chrome-extension://${id}/enroll.html`;
  const openPopup = async () => {
    const { targetId } = await call('Target.createTarget', { url: popup });
    return (await call('Target.attachToTarget', { targetId, flatten: true })).sessionId;
  };
  let extension = await openPopup();
  await waitFor(() => evaluate(extension, 'typeof window.probeRun === "function"'), 'extension page');
  // An unpacked profile may have cached the previous worker registration.
  await evaluate(extension, 'setTimeout(() => chrome.runtime.reload(), 0)');
  await delay(400);
  extension = await openPopup();
  await waitFor(() => evaluate(extension, 'document.querySelector("#service")?.options.length > 2'), 'production enrollment form');
  await call('Target.setAutoAttach', { autoAttach: true, waitForDebuggerOnStart: true, flatten: true, filter: [{ type: 'page', exclude: false }] });
  await evaluate(extension, `(() => {
    const kind = document.querySelector('#kind');
    kind.value = 'managed';
    kind.dispatchEvent(new Event('change'));
    document.querySelector('#service').value = 'youtube';
    document.querySelector('#enroll').click();
  })()`);
  diagnostics = () => evaluate(extension, `({ status: document.querySelector('#status')?.textContent, permissions: chrome.runtime.getManifest().permissions })`);
  await waitFor(() => evaluate(extension, 'document.querySelector("#status")?.textContent.includes("Connected to Mediacenter")'), 'production native registration');
  result.nativeConnected = true;
  await waitFor(async () => {
    if (fixtureError) throw fixtureError;
    if (broker.exitCode !== null) throw Error(`Broker exited before input: ${brokerError.trim()}`);
    return output.includes('input submitted') && catalogSession && await evaluate(catalogSession, 'document.querySelector("input")?.value === "synthetic café 🦊"');
  }, 'actual tab text');
  result.tabInputObserved = true;
  broker.stdin.end('\n');
  assert.deepEqual(await brokerExit, [0, null]);
  assert.ok(output.includes('closed'));
  await waitFor(async () => !(await call('Target.getTargets')).targetInfos.some(target => target.url === catalog), 'target tab closure');
  result.ownedTabClosed = true;
  result.passed = true;
} catch (error) {
  result.error = error.message;
  if (diagnostics) result.popup = await diagnostics().catch(() => null);
  await writeFile(path.join(base, 'production-runtime-stderr.log'), chromeLog, { mode: 0o600 });
  process.exitCode = 1;
} finally {
  if (broker?.exitCode === null && broker.signalCode === null) broker.kill();
  if (browser?.exitCode === null && browser.signalCode === null) {
    await call?.('Browser.close').catch(() => browser.kill());
    await Promise.race([browserExit, delay(2_000).then(() => browser.kill())]);
  }
  await rm(runtime, { recursive: true, force: true });
  await writeFile(path.join(base, 'production-runtime-result.json'), `${JSON.stringify(result, null, 2)}\n`);
  console.log(result);
}
