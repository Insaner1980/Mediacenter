// Fixed feasibility sequence. The broker supplies synthetic text only.
export async function runInputProof(debuggerApi, tabId, text, point, validate) {
  const send = (method, params) => {
    validate();
    return debuggerApi.sendCommand({ tabId }, method, params);
  };
  await send('Input.insertText', { text });
  const key = { key: 'Enter', code: 'Enter', windowsVirtualKeyCode: 13, text: '\r' };
  await send('Input.dispatchKeyEvent', { ...key, type: 'keyDown' });
  await send('Input.dispatchKeyEvent', { ...key, type: 'keyUp', text: '' });
  const click = { ...point, button: 'left', clickCount: 1 };
  await send('Input.dispatchMouseEvent', { ...click, type: 'mousePressed' });
  await send('Input.dispatchMouseEvent', { ...click, type: 'mouseReleased' });
}
