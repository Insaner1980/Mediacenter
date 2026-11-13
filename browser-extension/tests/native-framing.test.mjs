import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import { test } from 'node:test';
import { endianness } from 'node:os';

const host = process.env.MEDIACENTER_BROWSER_HOST;
const origin = 'chrome-extension://njhbcllpkbihpkioegopblabopnkhndk/';

test('native host rejects a foreign extension and oversized or truncated frames', { skip: !host }, () => {
  const oversized = Buffer.alloc(4);
  if (endianness() === 'LE') oversized.writeUInt32LE(65_537);
  else oversized.writeUInt32BE(65_537);
  for (const [caller, input] of [
    ['chrome-extension://other/', Buffer.alloc(0)],
    [origin, oversized],
    [origin, oversized.subarray(0, 2)],
  ]) {
    const run = spawnSync(host, [caller], { input, timeout: 5_000 });
    assert.ifError(run.error);
    assert.equal(run.status, 1);
    assert.equal(run.stdout.length, 0);
  }
});
