import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import { mkdirSync, mkdtempSync, rmSync } from 'node:fs';
import { test } from 'node:test';
import { endianness, tmpdir } from 'node:os';
import { join } from 'node:path';

const host = process.env.MEDIACENTER_BROWSER_HOST;
const origin = 'chrome-extension://njhbcllpkbihpkioegopblabopnkhndk/';

test('native host rejects a foreign extension and oversized or truncated frames', { skip: !host }, () => {
  // The browser-proof build uses the shared decoder without a live broker.
  const runtime = mkdtempSync(join(tmpdir(), 'mc-browser-framing-'));
  const oversized = Buffer.alloc(4);
  if (endianness() === 'LE') oversized.writeUInt32LE(65_537);
  else oversized.writeUInt32BE(65_537);
  try {
    mkdirSync(join(runtime, 'mediacenter-browser-proof'), { mode: 0o700 });
    for (const [caller, input] of [
      ['chrome-extension://other/', Buffer.alloc(0)],
      [origin, oversized],
      [origin, oversized.subarray(0, 2)],
    ]) {
      const run = spawnSync(host, [caller], {
        input, timeout: 5_000, env: { ...process.env, XDG_RUNTIME_DIR: runtime },
      });
      assert.ifError(run.error);
      assert.equal(run.status, 1);
      assert.equal(run.stdout.length, 0);
    }
  } finally {
    rmSync(runtime, { recursive: true });
  }
});
