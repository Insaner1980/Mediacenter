const keys = new Map([
  ['Enter', ['Enter', 'Enter', 13, '\r']],
  ['Escape', ['Escape', 'Escape', 27]],
  ['Tab', ['Tab', 'Tab', 9]],
  ['ShiftTab', ['Tab', 'Tab', 9, '', 8]],
  ['ArrowUp', ['ArrowUp', 'ArrowUp', 38]],
  ['ArrowDown', ['ArrowDown', 'ArrowDown', 40]],
  ['ArrowLeft', ['ArrowLeft', 'ArrowLeft', 37]],
  ['ArrowRight', ['ArrowRight', 'ArrowRight', 39]],
  ['Backspace', ['Backspace', 'Backspace', 8]],
  ['Space', [' ', 'Space', 32, ' ']],
]);

function requireFields(request, ...fields) {
  if (Object.keys(request).length !== fields.length + 1 || !fields.every(field => Object.hasOwn(request, field))) {
    throw Error('invalid_browser_input');
  }
}

function coordinate(value, length) {
  if (!Number.isFinite(value) || !Number.isFinite(length) || length < 1) throw Error('invalid_browser_input');
  return Math.max(0, Math.min(1, value)) * (length - 1);
}

function point(request, viewport) {
  if (!viewport || request.generation !== viewport.generation) throw Error('stale_viewport');
  return { x: coordinate(request.x, viewport.width), y: coordinate(request.y, viewport.height) };
}

export async function readViewport(debuggerApi, tabId, generation, validate) {
  validate();
  const { cssLayoutViewport } = await debuggerApi.sendCommand({ tabId }, 'Page.getLayoutMetrics');
  validate();
  const { clientWidth: width, clientHeight: height } = cssLayoutViewport ?? {};
  if (!Number.isFinite(width) || width < 1 || !Number.isFinite(height) || height < 1) throw Error('invalid_viewport');
  return { width, height, generation };
}

export async function requireSameViewport(debuggerApi, tabId, measured, validate) {
  const current = await readViewport(debuggerApi, tabId, measured.generation, validate);
  if (current.width !== measured.width || current.height !== measured.height) throw Error('stale_viewport');
  return current;
}

export async function dispatchBrowserInput(debuggerApi, tabId, request, viewport, validate, validateRelease = validate) {
  if (!request || typeof request !== 'object') throw Error('invalid_browser_input');
  const send = (method, params) => {
    validate();
    return debuggerApi.sendCommand({ tabId }, method, params);
  };
  const release = (method, params) => {
    validateRelease();
    return debuggerApi.sendCommand({ tabId }, method, params);
  };
  const mouse = (type, params) => send('Input.dispatchMouseEvent', { type, ...params });
  switch (request.type) {
    case 'text': {
      requireFields(request, 'text');
      if (typeof request.text !== 'string' || Array.from(request.text).length > 4096) throw Error('invalid_browser_input');
      await send('Input.insertText', { text: request.text });
      return;
    }
    case 'key': {
      requireFields(request, 'key');
      const definition = keys.get(request.key);
      if (!definition) throw Error('invalid_browser_key');
      const [key, code, windowsVirtualKeyCode, text = '', modifiers = 0] = definition;
      const params = { key, code, windowsVirtualKeyCode, modifiers };
      await send('Input.dispatchKeyEvent', { ...params, type: 'keyDown', text });
      await release('Input.dispatchKeyEvent', { ...params, type: 'keyUp' });
      return;
    }
    case 'pointer':
      requireFields(request, 'generation', 'x', 'y');
      await mouse('mouseMoved', point(request, viewport));
      return;
    case 'click': {
      requireFields(request, 'generation', 'x', 'y', 'button');
      if (request.button !== 'left' && request.button !== 'right') throw Error('invalid_browser_input');
      const params = { ...point(request, viewport), button: request.button, clickCount: 1 };
      await mouse('mousePressed', params);
      await release('Input.dispatchMouseEvent', { type: 'mouseReleased', ...params });
      return;
    }
    case 'scroll': {
      requireFields(request, 'generation', 'x', 'y', 'deltaX', 'deltaY');
      const { deltaX, deltaY } = request;
      if (![deltaX, deltaY].every(delta => Number.isFinite(delta) && Math.abs(delta) <= 4096)) {
        throw Error('invalid_browser_input');
      }
      await mouse('mouseWheel', { ...point(request, viewport), deltaX, deltaY });
      return;
    }
    default:
      throw Error('invalid_browser_input');
  }
}
