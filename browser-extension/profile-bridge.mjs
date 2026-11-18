import { isUuid } from './browser-session.mjs';
import { services } from './service-catalog.mjs';

const host = 'io.emma.mediacenter.browser';
const enrollmentKey = 'remoteEnrollment';
const alarmName = 'mediacenter-browser-reconnect';
const knownErrors = new Set(['stale_target', 'stale_viewport', 'origin_pending', 'bridge_unavailable', 'busy']);

function validSelection(kind, service) {
  return (kind === 'ordinary' && service === undefined)
    || (kind === 'managed' && service !== 'hbo_max' && services.has(service));
}

export class ProfileBridge {
  constructor(chrome, session) {
    this.chrome = chrome;
    this.session = session;
    this.actions = Promise.resolve();
    this.wakePages = new Map();
    chrome.tabs.onRemoved.addListener(tabId => this.forgetWakeTab(tabId));
    chrome.tabs.onUpdated.addListener((tabId, change) => {
      if (change.url !== undefined) this.forgetWakeTab(tabId);
    });
    chrome.runtime.onStartup.addListener(() => this.connect().catch(() => {}));
    chrome.runtime.onInstalled.addListener(() => this.connect().catch(() => {}));
    chrome.alarms.onAlarm.addListener(alarm => {
      if (alarm.name === alarmName) this.connect().catch(() => {});
    });
  }

  serialize(action) {
    const result = this.actions.then(action);
    this.actions = result.catch(() => {});
    return result;
  }

  async savedEnrollment() {
    const result = await this.chrome.storage.local.get(enrollmentKey);
    const enrollment = result[enrollmentKey];
    if (enrollment && isUuid(enrollment.profile) && validSelection(enrollment.kind, enrollment.service)) {
      const { profile, kind, service } = enrollment;
      return { profile, kind, ...(service ? { service } : {}) };
    }
    return null;
  }

  async status() {
    const enrollment = await this.savedEnrollment();
    return enrollment
      ? { enrolled: true, connected: this.connected === true, ...enrollment }
      : { enrolled: false, connected: false };
  }

  enroll(kind, service) {
    return this.serialize(async () => {
      if (!validSelection(kind, service)) throw Error('invalid_profile');
      const enrollment = await this.savedEnrollment();
      if (enrollment && (enrollment.kind !== kind || enrollment.service !== service)) throw Error('invalid_profile');
      if (!enrollment) {
        await this.chrome.storage.local.set({
          [enrollmentKey]: { profile: crypto.randomUUID(), kind, ...(service ? { service } : {}) },
        });
      }
      await this.connectSaved();
      return this.status();
    });
  }

  unenroll() {
    return this.serialize(async () => {
      await this.chrome.storage.local.remove(enrollmentKey);
      if (this.port) {
        const port = this.port;
        this.drop(port);
        port.disconnect();
      }
      await this.chrome.alarms.clear(alarmName);
      return this.status();
    });
  }

  connect() {
    return this.serialize(() => this.connectSaved());
  }

  async connectSaved() {
    if (this.port) return;
    const enrollment = await this.savedEnrollment();
    if (!enrollment) return;
    if (!await this.chrome.alarms.get(alarmName)) {
      await this.chrome.alarms.create(alarmName, { periodInMinutes: 1 });
    }

    let port;
    try {
      port = this.chrome.runtime.connectNative(host);
      this.port = port;
      port.onDisconnect.addListener(() => {
        // Chrome sets lastError for a missing host or failed broker connection.
        void this.chrome.runtime.lastError;
        this.drop(port);
      });
      port.onMessage.addListener(message => this.receive(port, enrollment, message));
      this.post(port, { type: 'hello', ...enrollment });
    } catch {
      this.drop(port);
    }
  }

  wake(nonce, tabId) {
    return this.serialize(async () => {
      const enrollment = await this.savedEnrollment();
      if (!enrollment) throw Error('not_enrolled');
      if (enrollment.kind !== 'managed' || !isUuid(nonce) || !Number.isInteger(tabId)) {
        throw Error('invalid_profile');
      }
      await this.connectSaved();
      if (!this.port || this.wakePages.has(nonce) || !this.post(this.port, { type: 'wake', nonce })) {
        throw Error('bridge_unavailable');
      }
      this.wakePages.set(nonce, tabId);
    });
  }

  forgetWakeTab(tabId) {
    for (const [nonce, pendingTab] of this.wakePages) {
      if (pendingTab === tabId) this.wakePages.delete(nonce);
    }
  }

  receive(port, enrollment, message) {
    if (this.port !== port) return;
    if (message?.type === 'welcome') {
      this.connected = true;
      return;
    }
    if (!this.connected || !isUuid(message?.id) || !message.command) return;
    const { id, command } = message;
    this.session.command(command, enrollment).then(
      () => {
        if (this.post(port, { type: 'reply', id, ok: true }) && command.type === 'launch') {
          const tabId = this.wakePages.get(command.nonce);
          this.wakePages.delete(command.nonce);
          if (tabId !== undefined) this.chrome.tabs.remove(tabId).catch(() => {});
        }
      },
      failure => {
        if (this.port === port && command.type === 'launch') this.wakePages.delete(command.nonce);
        this.post(port, {
          type: 'reply', id, ok: false,
          error: knownErrors.has(failure.message) ? failure.message : 'bridge_unavailable',
        });
      },
    );
  }

  publish(state) {
    if (this.connected) this.post(this.port, state);
  }

  post(port, message) {
    if (!port || this.port !== port) return false;
    try {
      port.postMessage(message);
      return true;
    } catch {
      this.drop(port);
      port.disconnect();
      return false;
    }
  }

  drop(port) {
    if (!port || this.port !== port) return;
    this.port = null;
    this.connected = false;
    this.wakePages.clear();
    this.session.disconnect();
  }
}
