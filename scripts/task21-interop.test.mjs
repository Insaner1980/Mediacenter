import { test } from 'node:test';
import { Buffer } from 'node:buffer';
import assert from 'node:assert/strict';
import { cpSync, existsSync, mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { dirname, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { spawnSync } from 'node:child_process';

const root = resolve(dirname(fileURLToPath(import.meta.url)), '..');
const plan = '.superpowers/sdd/2026-11-09-remote-mediacenter-sofa-control';
const consumer = 'app/src/test/java/com/emmah/colordock/remote/mediacenter/RustInteropTest.kt';
const resource = 'app/src/test/resources/mediacenter-rust-interop-v1.json';
const kotlin = 'app/src/main/java/com/emmah/colordock/remote/mediacenter';

function write(path, contents, options) {
  mkdirSync(dirname(path), { recursive: true });
  writeFileSync(path, contents, options);
}

function isolated(t) {
  const temporary = mkdtempSync(resolve(tmpdir(), 'task21-guard-'));
  t.after(() => rmSync(temporary, { recursive: true, force: true }));
  const mc = resolve(temporary, 'MC');
  const remote = resolve(temporary, 'Remote');
  for (const path of [
    'scripts/task21-interop.mjs', 'scripts/task21-interop.test.mjs', 'protocol/remote/v1',
    'src-tauri/Cargo.toml', 'src-tauri/Cargo.lock', 'src-tauri/build.rs', 'src-tauri/src/remote',
  ]) {
    mkdirSync(dirname(resolve(mc, path)), { recursive: true });
    cpSync(resolve(root, path), resolve(mc, path), { recursive: true });
  }
  for (const path of ['protocol/RemoteJson.kt', 'protocol/Messages.kt', 'protocol/StrictJson.kt', 'RemoteStateReducer.kt']) {
    write(resolve(remote, kotlin, path), '// Synthetic Android source for the freshness guard\n');
  }
  write(resolve(remote, consumer), '// Synthetic Kotlin consumer\n');

  // Exercise the actual staging CLI while controlling when its external Cargo process finishes.
  const fixtures = JSON.parse(readFileSync(resolve(mc, 'protocol/remote/v1/fixtures.json')));
  write(resolve(mc, 'test-corpus.json'), JSON.stringify({
    format: 'mediacenter-rust-interop-v1',
    requests: fixtures.requests,
    replies: fixtures.replies,
    snapshots: [{}, {}],
    invalidRequests: Array(44).fill({}),
    invalidReplies: [{}, {}],
    dispatcher: { effectCount: 1 },
  }));
  const bin = resolve(temporary, 'bin');
  write(resolve(bin, 'cargo'), [
    '#!/bin/sh',
    'cp test-corpus.json "$MEDIACENTER_TASK21_CORPUS_OUT" || exit 1',
    'if [ -n "$TASK21_CHANGE_SOURCE" ]; then printf "\\n// changed during export\\n" >> "$TASK21_CHANGE_SOURCE"; fi',
    '',
  ].join('\n'), { mode: 0o700 });

  const targetPlan = resolve(mc, plan);
  const fresh = resolve(targetPlan, 'task-21-fresh-corpus.json');
  const run = (action, env = {}) => spawnSync(process.execPath, [resolve(mc, 'scripts/task21-interop.mjs'), action], {
    cwd: mc,
    encoding: 'utf8',
    env: { ...process.env, PATH: `${bin}:${process.env.PATH}`, ...env },
  });
  const prepareAndInstall = () => {
    const prepared = run('prepare');
    assert.equal(prepared.status, 0, prepared.stderr);
    write(resolve(remote, resource), readFileSync(fresh));
  };
  return { mc, remote, targetPlan, fresh, run, prepareAndInstall };
}

test('prepared bytes verify and installed tampering fails with no protected-tree writes', t => {
  const fixture = isolated(t);
  fixture.prepareAndInstall();
  const good = fixture.run('verify');
  assert.equal(good.status, 0, good.stderr);

  const path = resolve(fixture.remote, resource);
  const tampered = Buffer.concat([readFileSync(path), Buffer.from('\n')]);
  write(path, tampered);
  const bad = fixture.run('verify');
  assert.notEqual(bad.status, 0);
  assert.match(bad.stderr, /Installed corpus differs from fresh exporter output/);
  assert.deepEqual(readFileSync(path), tampered);
});

test('prepare does not publish a corpus or stage files if a source changes during export', t => {
  const fixture = isolated(t);
  const changed = fixture.run('prepare', { TASK21_CHANGE_SOURCE: 'src-tauri/src/remote/protocol.rs' });
  assert.notEqual(changed.status, 0);
  assert.match(changed.stderr, /source hashes changed during export/);
  assert.equal(existsSync(fixture.targetPlan), false);
});

for (const input of ['src-tauri/src/remote/protocol.rs', 'src-tauri/Cargo.toml', 'src-tauri/Cargo.lock', 'src-tauri/build.rs']) {
  test(`verify rejects a corpus after ${input} changes`, t => {
    const fixture = isolated(t);
    fixture.prepareAndInstall();
    const path = resolve(fixture.mc, input);
    write(path, Buffer.concat([readFileSync(path), Buffer.from('\n')]));
    const stale = fixture.run('verify');
    assert.notEqual(stale.status, 0);
    assert.match(stale.stderr, /source hashes changed/);
  });
}
