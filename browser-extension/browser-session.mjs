import { dispatchBrowserInput, readViewport } from './input.mjs';
import { services } from './service-catalog.mjs';

const uuid = /^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/;
const sequence = /^(0|[1-9][0-9]{0,19})$/;
const motionInterval = 1000 / 30;
const center = { x: 0.5, y: 0.5 };
const markerColor = { r: 255, g: 255, b: 255, a: 1 };
const markerOutline = { r: 0, g: 0, b: 0, a: 1 };

export function isUuid(value) {
  return typeof value === 'string' && uuid.test(value);
}

function originOf(address) {
  if (typeof address !== 'string' || !URL.canParse(address)) return null;
  const url = new URL(address);
  if (url.protocol !== 'https:' || url.username || url.password) return null;
  return url.origin;
}

function exactFields(value, fields) {
  return value !== null && typeof value === 'object' && Object.keys(value).length === fields.length
    && fields.every(field => Object.hasOwn(value, field));
}

function sameSize(left, right) {
  return left?.width === right.width && left?.height === right.height;
}

export class BrowserSession {
  constructor(chrome, emit) {
    this.chrome = chrome;
    this.emit = emit;
    this.generation = 0n;
    this.usedNonces = new Set();
    chrome.tabs.onRemoved.addListener(tabId => this.tabRemoved(tabId));
    chrome.tabs.onReplaced.addListener((_, previous) => this.loseTab(previous, true));
    chrome.debugger.onDetach.addListener(({ tabId }, reason) => {
      if (reason === 'target_closed') this.tabRemoved(tabId);
      else this.loseTab(tabId, false);
    });
    chrome.tabs.onUpdated.addListener((tabId, change, tab) => {
      const target = this.target;
      if (target?.tabId !== tabId) return;
      if (change.status === 'loading' || change.url !== undefined) this.suspend(target);
      if ((change.status === 'complete' || change.url !== undefined) && tab.status === 'complete' && !tab.pendingUrl) {
        this.refresh(target).catch(() => {});
      }
    });
    chrome.debugger.onEvent.addListener(({ tabId }, method, params) => {
      const target = this.target;
      if (target?.tabId !== tabId) return;
      if (method === 'Page.frameResized' && target.ready) {
        this.refresh(target).catch(() => {});
      }
      if (method === 'Page.frameStartedNavigating' && params.frameId === target.frameId) {
        this.suspend(target);
      }
      if ((method === 'Page.frameNavigated' && !params.frame.parentId)
        || (method === 'Page.navigatedWithinDocument' && params.frameId === target.frameId)) {
        this.suspend(target);
        this.refresh(target).catch(() => {});
      }
    });
    chrome.tabs.onActivated.addListener(({ tabId, windowId }) => {
      const target = this.target;
      if (target && target.windowId === windowId && target.tabId !== tabId) this.hideMarker(target);
    });
    chrome.windows.onFocusChanged.addListener(windowId => {
      const target = this.target;
      if (target && target.windowId !== windowId) this.hideMarker(target);
    });
    chrome.windows.onBoundsChanged.addListener(({ id }) => {
      const target = this.target;
      if (target?.ready && target.windowId === id) this.refresh(target).catch(() => {});
    });
  }

  publish(target, closed = false) {
    this.emit({
      type: 'state', session: target.session, generation: target.generation,
      origin: target.origin, pendingOrigin: target.pendingOrigin, ready: target.ready,
      ...(closed ? { closed: true } : {}),
    });
  }

  advance(target) {
    this.hideMarker(target);
    this.cancelMotion(target);
    target.generation = String(++this.generation);
    target.lastSequence = -1n;
    target.point = center;
  }

  suspend(target) {
    target.navigation += 1;
    if (!target.ready && target.origin === null && target.pendingOrigin === null) return;
    this.advance(target);
    Object.assign(target, { origin: null, pendingOrigin: null, ready: false, viewport: null });
    this.publish(target);
  }

  loseTab(tabId, detach) {
    const target = this.target;
    if (target?.tabId === tabId) this.finish(target, detach);
  }

  tabRemoved(tabId) {
    const target = this.target;
    if (!target) return;
    if (target.original?.tabId === tabId) this.finish(target);
    else if (target.tabId === tabId) {
      if (target.original) this.restoreOriginal(target).catch(() => {});
      else this.finish(target, false);
    }
  }

  finish(target, detach = true) {
    if (this.target !== target) return;
    this.target = null;
    this.advance(target);
    Object.assign(target, { origin: null, pendingOrigin: null, ready: false, viewport: null });
    this.publish(target, true);
    if (detach && target.attached) this.chrome.debugger.detach({ tabId: target.tabId }).catch(() => {});
  }

  disconnect() {
    this.launchCancelled = true;
    if (this.target) this.finish(this.target);
  }

  assertCurrent(target, generation = target.generation) {
    if (this.target !== target || generation !== target.generation) throw Error('stale_target');
    return target;
  }

  async launch(command, profile) {
    const { url, session, nonce } = command;
    const entry = [...services].find(([, service]) => service.url === url);
    if (!entry || !isUuid(session) || !isUuid(nonce) || this.usedNonces.has(nonce)
      || (entry[0] === 'hbo_max' ? profile.kind !== 'ordinary'
        : profile.kind !== 'managed' || profile.service !== entry[0])) {
      throw Error('bridge_unavailable');
    }
    if (this.target || this.launching) throw Error('bridge_unavailable');
    this.usedNonces.add(nonce);
    this.launching = true;
    this.launchCancelled = false;
    let tab;
    let target;
    try {
      // Only the tab returned by creating this window becomes a control target.
      const window = await this.chrome.windows.create({ url: 'about:blank', focused: true });
      [tab] = window?.tabs ?? [];
      if (window?.tabs?.length !== 1 || !Number.isInteger(tab?.id) || this.launchCancelled) {
        throw Error('bridge_unavailable');
      }
      const allowed = new Set([originOf(url), ...entry[1].origins]);
      target = {
        session, tabId: tab.id, windowId: window.id, allowed,
        navigation: 0, origin: null, pendingOrigin: null, ready: false,
        viewport: null, attached: false, lastMotion: -Infinity,
      };
      this.target = target;
      this.advance(target);
      await this.attach(target);
      this.publish(target);
      await this.chrome.tabs.update(target.tabId, { url });
      this.assertCurrent(target);
      this.refresh(target).catch(() => {});
    } catch {
      if (target) this.finish(target);
      if (Number.isInteger(tab?.id)) await this.chrome.tabs.remove(tab.id).catch(() => {});
      throw Error('bridge_unavailable');
    } finally {
      this.launching = false;
    }
  }

  async attach(target) {
    const debuggee = { tabId: target.tabId };
    await this.chrome.debugger.attach(debuggee, '1.3');
    if (this.target !== target) {
      await this.chrome.debugger.detach(debuggee).catch(() => {});
      throw Error('stale_target');
    }
    target.attached = true;
    await this.chrome.debugger.sendCommand(debuggee, 'Page.enable');
    this.assertCurrent(target);
  }

  async activeRelatedPopup(target) {
    if (!target || target.original || !target.attached || this.launching) return null;
    const [tab] = await this.chrome.tabs.query({ active: true, currentWindow: true });
    if (this.target !== target || tab?.openerTabId !== target.tabId || !originOf(tab.url)) return null;
    return tab;
  }

  async canAdoptActivePopup() {
    return Boolean(await this.activeRelatedPopup(this.target));
  }

  async adoptActivePopup() {
    const original = this.target;
    const tab = await this.activeRelatedPopup(original);
    if (!tab || this.target !== original) throw Error('stale_target');
    this.hideMarker(original);
    this.cancelMotion(original);
    const target = {
      original, session: original.session, tabId: tab.id, windowId: tab.windowId,
      allowed: new Set(), navigation: 0, origin: null, pendingOrigin: null, ready: false,
      viewport: null, attached: false, lastMotion: -Infinity,
    };
    this.target = target;
    this.advance(target);
    this.publish(target);
    const detach = this.chrome.debugger.detach({ tabId: original.tabId }).catch(() => {});
    target.originalDetached = detach;
    original.attached = false;
    try {
      await detach;
      this.assertCurrent(target);
      await this.attach(target);
      await this.refresh(target);
    } catch {
      if (this.target === target) await this.restoreOriginal(target, true);
      throw Error('bridge_unavailable');
    }
  }

  async restoreOriginal(popup, detach = false) {
    if (this.target !== popup) return;
    this.hideMarker(popup);
    this.cancelMotion(popup);
    const target = popup.original;
    this.target = target;
    this.advance(target);
    Object.assign(target, { origin: null, pendingOrigin: null, ready: false, viewport: null, overlay: false });
    this.publish(target);
    try {
      if (detach && popup.attached) await this.chrome.debugger.detach({ tabId: popup.tabId }).catch(() => {});
      await popup.originalDetached;
      this.assertCurrent(target);
      await this.attach(target);
      await this.refresh(target);
    } catch {
      this.finish(target);
    }
  }

  async snapshot(target, generation) {
    const navigation = target.navigation;
    const validate = () => {
      this.assertCurrent(target, generation);
      if (target.navigation !== navigation) throw Error('stale_target');
    };
    validate();
    const tab = await this.chrome.tabs.get(target.tabId);
    validate();
    target.windowId = tab.windowId;
    if (tab.status !== 'complete' || tab.pendingUrl) return null;
    const { frameTree } = await this.chrome.debugger.sendCommand({ tabId: target.tabId }, 'Page.getFrameTree');
    validate();
    const frame = frameTree?.frame;
    const origin = originOf(frame?.url);
    if (!frame || frame.unreachableUrl || !origin || originOf(tab.url) !== origin) return null;
    const viewport = await readViewport(this.chrome.debugger, target.tabId, generation, validate);
    return { origin, frame, viewport, navigation };
  }

  updateSnapshot(target, snapshot) {
    const { origin, frame, viewport } = snapshot;
    const pendingOrigin = target.allowed.has(origin) ? null : origin;
    const changedDocument = target.origin !== null && (target.origin !== origin
      || target.frameId !== frame.id || target.loaderId !== frame.loaderId);
    const resized = target.viewport && !sameSize(target.viewport, viewport);
    const changed = changedDocument || resized || target.pendingOrigin !== pendingOrigin
      || target.ready !== (pendingOrigin === null);
    if (changedDocument || resized) this.advance(target);
    Object.assign(target, {
      origin, pendingOrigin, ready: pendingOrigin === null,
      frameId: frame.id, loaderId: frame.loaderId,
      viewport: { ...viewport, generation: target.generation },
    });
    if (changed) this.publish(target);
  }

  async refresh(target) {
    if (this.target !== target || !target.attached) return;
    const generation = target.generation;
    const snapshot = await this.snapshot(target, generation);
    if (snapshot && this.target === target && target.generation === generation && target.navigation === snapshot.navigation) {
      this.updateSnapshot(target, snapshot);
    }
  }

  async requireSnapshot(target, generation) {
    const snapshot = await this.snapshot(target, generation);
    this.assertCurrent(target, generation);
    if (!snapshot) {
      this.suspend(target);
      throw Error('stale_target');
    }
    if (target.navigation !== snapshot.navigation) throw Error('stale_target');
    this.updateSnapshot(target, snapshot);
    if (target.generation !== generation) {
      throw Error(target.ready ? 'stale_viewport' : 'stale_target');
    }
    return snapshot;
  }

  async command(command, profile) {
    if (command?.type === 'launch') {
      if (!exactFields(command, ['type', 'url', 'session', 'nonce'])) throw Error('bridge_unavailable');
      return this.launch(command, profile);
    }
    const target = this.target;
    const { type, session, generation, input } = command ?? {};
    const fields = ['type', 'session', 'generation', ...(type === 'input' ? ['input'] : [])];
    if (!exactFields(command, fields)) throw Error('bridge_unavailable');
    if (!target || target.session !== session) throw Error('stale_target');
    this.assertCurrent(target, generation);
    switch (type) {
      case 'input':
        return this.admitInput(target, generation, input);
      case 'approve': {
        const origin = target.pendingOrigin;
        if (!origin) throw Error('stale_target');
        await this.requireSnapshot(target, generation);
        if (origin !== target.pendingOrigin) throw Error('stale_target');
        target.allowed.add(origin);
        this.advance(target);
        target.viewport = { ...target.viewport, generation: target.generation };
        Object.assign(target, { pendingOrigin: null, ready: true });
        this.publish(target);
        return;
      }
      case 'back':
      case 'reload':
        this.suspend(target);
        try {
          if (type === 'back') await this.chrome.tabs.goBack(target.tabId);
          else await this.chrome.tabs.reload(target.tabId);
        } catch {
          this.refresh(target).catch(() => {});
          throw Error('bridge_unavailable');
        }
        return;
      case 'close':
        this.suspend(target);
        try {
          await this.chrome.tabs.remove(target.original?.tabId ?? target.tabId);
        } catch {
          this.refresh(target).catch(() => {});
          throw Error('bridge_unavailable');
        }
        this.finish(target, Boolean(target.original));
        return;
      case 'focus': {
        const tab = await this.chrome.tabs.update(target.tabId, { active: true });
        this.assertCurrent(target, generation);
        await this.chrome.windows.update(tab.windowId, { focused: true });
        return;
      }
      default:
        throw Error('bridge_unavailable');
    }
  }

  requireReady(target, generation) {
    this.assertCurrent(target, generation);
    if (target.pendingOrigin) throw Error('origin_pending');
    if (!target.ready) throw Error('stale_target');
  }

  admitInput(target, generation, input) {
    this.requireReady(target, generation);
    if (input?.type === 'pointer' || input?.type === 'scroll') {
      return this.queueMotion(target, generation, input);
    }
    if (!['click', 'key', 'text'].includes(input?.type)) throw Error('bridge_unavailable');
    this.cancelMotion(target);
    return this.dispatch(target, generation, input);
  }

  queueMotion(target, generation, input) {
    const pointer = input.type === 'pointer';
    const fields = pointer ? ['x', 'y'] : ['deltaX', 'deltaY'];
    if (!exactFields(input, ['type', 'inputSequence', ...fields])
      || typeof input.inputSequence !== 'string' || !sequence.test(input.inputSequence)
      || !fields.every(field => Number.isFinite(input[field]))) throw Error('bridge_unavailable');
    const order = BigInt(input.inputSequence);
    if (order <= target.lastSequence) throw Error('stale_target');
    target.lastSequence = order;
    this.cancelMotion(target);
    return new Promise((resolve, reject) => {
      target.motion = { generation, input, resolve, reject };
      this.scheduleMotion(target);
    });
  }

  cancelMotion(target) {
    clearTimeout(target.motionTimer);
    target.motionTimer = undefined;
    target.motion?.reject(Error('bridge_unavailable'));
    target.motion = undefined;
  }

  scheduleMotion(target) {
    if (target.inputBusy || !target.motion || target.motionTimer !== undefined) return;
    const delay = Math.max(0, motionInterval - (performance.now() - target.lastMotion));
    target.motionTimer = setTimeout(() => {
      const { generation, input, resolve, reject } = target.motion;
      target.motion = undefined;
      target.motionTimer = undefined;
      target.lastMotion = performance.now();
      const { type, inputSequence: _, ...fields } = input;
      const request = type === 'pointer' ? fields : { ...target.point, ...fields };
      this.dispatch(target, generation, { type, generation, ...request }).then(resolve, reject);
    }, Math.ceil(delay));
  }

  async dispatch(target, generation, input) {
    if (target.inputBusy) throw Error('bridge_unavailable');
    target.inputBusy = true;
    const validate = () => this.requireReady(target, generation);
    try {
      await this.requireSnapshot(target, generation);
      validate();
      let request = input;
      if (input.type === 'click') {
        if (!exactFields(input, ['type', 'x', 'y', 'button'])) throw Error('bridge_unavailable');
        request = { ...input, generation };
      }
      await dispatchBrowserInput(this.chrome.debugger, target.tabId, request, target.viewport, validate);
      if (input.type === 'pointer' || input.type === 'click') {
        validate();
        target.point = { x: input.x, y: input.y };
        await this.showMarker(target, generation);
      }
    } catch (error) {
      if (['stale_target', 'stale_viewport', 'origin_pending'].includes(error.message)) throw error;
      throw Error('bridge_unavailable');
    } finally {
      target.inputBusy = false;
      this.scheduleMotion(target);
    }
  }

  async showMarker(target, generation) {
    const debuggee = { tabId: target.tabId };
    try {
      const tab = await this.chrome.tabs.get(target.tabId);
      this.requireReady(target, generation);
      if (tab.active === false) return;
      if (!target.overlay) {
        await this.chrome.debugger.sendCommand(debuggee, 'DOM.enable');
        this.requireReady(target, generation);
        await this.chrome.debugger.sendCommand(debuggee, 'Overlay.enable');
        this.requireReady(target, generation);
        target.overlay = true;
      }
      const { width, height } = target.viewport;
      const x = Math.round(Math.max(0, Math.min(1, target.point.x)) * (width - 1));
      const y = Math.round(Math.max(0, Math.min(1, target.point.y)) * (height - 1));
      await this.chrome.debugger.sendCommand(debuggee, 'Overlay.highlightRect', {
        x: x - 4, y: y - 4, width: 8, height: 8, color: markerColor, outlineColor: markerOutline,
      });
      clearTimeout(target.markerTimer);
      target.markerTimer = setTimeout(() => this.hideMarker(target), 1_500);
    } catch {
      // The marker is cosmetic. The input command has already completed.
    }
  }

  hideMarker(target) {
    clearTimeout(target.markerTimer);
    if (target.overlay) {
      this.chrome.debugger.sendCommand({ tabId: target.tabId }, 'Overlay.hideHighlight').catch(() => {});
    }
  }
}
