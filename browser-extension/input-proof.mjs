import { dispatchBrowserInput, readViewport } from './input.mjs';

// Fixed feasibility sequence. The broker supplies synthetic text only.
export async function runInputProof(debuggerApi, tabId, text, point, validate) {
  const viewport = await readViewport(debuggerApi, tabId, 'proof', validate);
  const send = request => dispatchBrowserInput(debuggerApi, tabId, request, viewport, validate);
  await send({ type: 'text', text });
  await send({ type: 'key', key: 'Enter' });
  await send({ type: 'click', generation: viewport.generation, x: point.x / Math.max(1, viewport.width - 1), y: point.y / Math.max(1, viewport.height - 1), button: 'left' });
}
