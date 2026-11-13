import { BrowserSession } from './browser-session.mjs';
import { readViewport, requireSameViewport } from './input.mjs';
import { runInputProof } from './input-proof.mjs';
import { ProfileBridge } from './profile-bridge.mjs';
import { TargetRegistry } from './target-registry.mjs';

const browserSession = new BrowserSession(chrome, state => profileBridge.publish(state));
const profileBridge = new ProfileBridge(chrome, browserSession);
profileBridge.connect().catch(() => {});

const registry = new TargetRegistry();
const worker = crypto.randomUUID();
const proofRevision = 7;
const host = 'io.emma.mediacenter.browser_proof';
let preparing = false;
let inputGrant;

function invalidate(tabId) {
  if (registry.target?.tabId !== tabId) return;
  registry.invalidate();
  inputGrant = undefined;
}

chrome.debugger.onDetach.addListener(({ tabId }) => invalidate(tabId));
chrome.tabs.onRemoved.addListener(invalidate);
chrome.tabs.onReplaced.addListener((_, previous) => invalidate(previous));
chrome.tabs.onUpdated.addListener((tabId, change) => {
  if (registry.target?.tabId === tabId && (change.status === 'loading' || change.url)) {
    invalidate(tabId);
    chrome.debugger.detach({ tabId }).catch(() => {});
  }
});

async function prepare(nonce) {
  if (preparing || registry.target || typeof nonce !== 'string' || nonce.length !== 36) {
    throw Error('target_busy');
  }
  preparing = true;
  let tab;
  let attached = false;
  let onUpdated;
  let onRemoved;
  let timer;
  let phase = 'proof_storage';
  try {
    const { profile } = await chrome.storage.local.get('profile');
    registry.enroll(profile);
    let grant;
    try {
      grant = await chrome.runtime.sendNativeMessage(host, { profile, nonce });
    } catch (error) {
      const unavailable = /not found/i.test(error.message);
      throw Error(unavailable ? 'native_host_not_found' : 'native_host_no_reply');
    }
    phase = 'proof_grant';
    const url = new URL(grant.url);
    if (url.protocol !== 'http:' || url.hostname !== '127.0.0.1' || !url.port
      || url.pathname !== '/catalog' || url.search || url.hash || url.username || url.password) {
      throw Error('invalid_catalog');
    }
    registry.issue(profile, nonce, url.origin, Date.now() + 2_000);
    registry.launch(profile, nonce, Date.now());
    phase = 'proof_tab_create';
    tab = await chrome.tabs.create({ active: true, url: url.href });
    phase = 'proof_tab_load';
    await new Promise((resolve, reject) => {
      onUpdated = (id, change, current) => {
        if (id === tab.id && change.status === 'complete' && current.url === url.href) resolve();
      };
      onRemoved = (id) => {
        if (id === tab.id) reject(Error('tab_closed'));
      };
      chrome.tabs.onUpdated.addListener(onUpdated);
      chrome.tabs.onRemoved.addListener(onRemoved);
      timer = setTimeout(() => reject(Error('catalog_timeout')), 5_000);
      chrome.tabs.get(tab.id).then(current => onUpdated(tab.id, current, current), reject);
    });
    phase = 'proof_attach';
    const generation = crypto.randomUUID();
    registry.attach(tab.id, grant.session, generation);
    try {
      await chrome.debugger.attach({ tabId: tab.id }, '1.3');
    } catch {
      throw Error('debugger_attach_failed');
    }
    attached = true;
    registry.validate(grant.session, generation, tab.id);
    return { tabId: tab.id, session: grant.session, generation, text: grant.text };
  } catch (error) {
    registry.invalidate();
    if (attached) await chrome.debugger.detach({ tabId: tab.id }).catch(() => {});
    if (tab) await chrome.tabs.remove(tab.id).catch(() => {});
    if (error instanceof TypeError) throw Error(phase);
    throw error;
  } finally {
    if (onUpdated) chrome.tabs.onUpdated.removeListener(onUpdated);
    if (onRemoved) chrome.tabs.onRemoved.removeListener(onRemoved);
    clearTimeout(timer);
    preparing = false;
  }
}

async function handle(message) {
  switch (message.type) {
    case 'remoteStatus': {
      const status = await profileBridge.status();
      return { ...status, canAdoptPopup: status.connected && await browserSession.canAdoptActivePopup() };
    }
    case 'remoteEnroll':
      return profileBridge.enroll(message.kind, message.service);
    case 'remoteUnenroll':
      return profileBridge.unenroll();
    case 'remoteAdoptPopup':
      if (!profileBridge.connected) throw Error('bridge_unavailable');
      await browserSession.adoptActivePopup();
      return profileBridge.status();
    case 'enroll': {
      if (preparing || registry.target) throw Error('target_busy');
      const profile = crypto.randomUUID();
      await chrome.storage.local.set({ profile });
      registry.enroll(profile);
      return { enrolled: true, profile };
    }
    case 'probe':
      return { admitted: true, extensionId: chrome.runtime.id, worker, proofRevision };
    case 'prepareProof': {
      const { text, ...target } = await prepare(message.nonce);
      inputGrant = { text, viewport: undefined };
      return target;
    }
    case 'measureProof': {
      const { session, generation, tabId } = message;
      const validate = () => registry.validate(session, generation, tabId);
      const viewport = await readViewport(chrome.debugger, tabId, generation, validate);
      if (!inputGrant) throw Error('grant_consumed');
      inputGrant.viewport = viewport;
      return { measured: true };
    }
    case 'inputProof': {
      const { session, generation, tabId } = message;
      const validate = () => registry.validate(session, generation, tabId);
      const target = validate();
      const tab = await chrome.tabs.get(tabId);
      if (tab.status !== 'complete' || new URL(tab.url).origin !== target.origin) throw Error('stale_target');
      validate();
      const grant = inputGrant;
      if (!grant) throw Error('grant_consumed');
      inputGrant = undefined;
      if (grant.viewport) await requireSameViewport(chrome.debugger, tabId, grant.viewport, validate);
      await runInputProof(chrome.debugger, tabId, grant.text, { x: 80, y: 58 }, validate);
      return { submitted: true };
    }
    default:
      throw Error('unsupported');
  }
}

chrome.runtime.onMessage.addListener((message, sender, reply) => {
  if (sender.id !== chrome.runtime.id || sender.url !== chrome.runtime.getURL('enroll.html')) return false;
  if (!message || typeof message !== 'object') return false;
  handle(message).then(reply, error => {
    const known = ['native_host_not_found', 'native_host_no_reply', 'invalid_catalog', 'catalog_timeout', 'debugger_attach_failed', 'stale_target', 'stale_viewport', 'stale_launch', 'profile_required', 'tab_closed'];
    reply({ error: (known.includes(error.message) || /^proof_(storage|grant|tab_create|tab_load|attach)$/.test(error.message)) ? error.message : 'bridge_unavailable' });
  });
  return true;
});
