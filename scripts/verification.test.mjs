import { test } from 'node:test';
import assert from 'node:assert/strict';
import { cpSync, existsSync, mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { dirname, resolve } from 'node:path';
import { tmpdir } from 'node:os';
import { fileURLToPath } from 'node:url';
import { spawnSync } from 'node:child_process';

const root = resolve(dirname(fileURLToPath(import.meta.url)), '..');
const protocol = 'protocol/remote/v1';
const generated = 'src-tauri/src/remote/protocol_methods.rs';

function fixture(t, paths) {
  const checkout = mkdtempSync(resolve(tmpdir(), 'mediacenter-verification-'));
  t.after(() => rmSync(checkout, { recursive: true, force: true }));
  for (const path of paths) {
    const target = resolve(checkout, path);
    mkdirSync(dirname(target), { recursive: true });
    cpSync(resolve(root, path), target, { recursive: true });
  }
  const run = (executable, ...args) => spawnSync(executable, args, { cwd: checkout, encoding: 'utf8' });
  return { checkout, run };
}

test('an incomplete bundle option rejects before publishing a manifest', t => {
  const { checkout, run } = fixture(t, ['scripts/verify-remote-contract.mjs', protocol]);
  const manifest = resolve(checkout, protocol, 'manifest.json');
  writeFileSync(manifest, '{}\n');
  for (const args of [['--update-manifest', '--bundle'], ['--bundle', '--update-manifest']]) {
    const result = run(process.execPath, 'scripts/verify-remote-contract.mjs', ...args);
    assert.notEqual(result.status, 0);
    assert.match(result.stderr, /--bundle requires a path/);
    assert.equal(readFileSync(manifest, 'utf8'), '{}\n');
    assert.equal(existsSync(resolve(checkout, '--update-manifest')), false);
  }
  const result = run(process.execPath, 'scripts/verify-remote-contract.mjs', '--update-manifest');
  assert.equal(result.status, 0, result.stderr);
  assert.equal(JSON.parse(readFileSync(manifest, 'utf8')).requestMethods, 88);
});

test('Kotlin-only checking tolerates whitespace and never changes generated Rust', t => {
  const { checkout, run } = fixture(t, ['scripts/generate-remote-contract.py', protocol, generated]);
  const script = 'scripts/generate-remote-contract.py';
  const rust = resolve(checkout, generated);
  const generatedKotlin = run('python3', script, '--kotlin-output', 'kotlin');
  assert.equal(generatedKotlin.status, 0, generatedKotlin.stderr);
  writeFileSync(rust, '// existing Rust output\n');
  const kotlin = resolve(checkout, 'kotlin/RemoteJson.kt');
  const original = readFileSync(kotlin, 'utf8');
  const spaced = original.replace('root.opt("v") != 1', 'root.opt( "v" ) != 1');
  assert.notEqual(spaced, original);
  writeFileSync(kotlin, spaced);
  const result = run('python3', script, '--kotlin-check', 'kotlin');
  assert.equal(result.status, 0, result.stderr);
  assert.ok(readFileSync(rust, 'utf8') === '// existing Rust output\n', 'Kotlin-only check rewrote Rust output');
});

test('the Kotlin output check rejects a split comparison operator', t => {
  const { checkout, run } = fixture(t, ['scripts/generate-remote-contract.py', protocol, generated]);
  const script = 'scripts/generate-remote-contract.py';
  const generatedKotlin = run('python3', script, '--kotlin-output', 'kotlin');
  assert.equal(generatedKotlin.status, 0, generatedKotlin.stderr);
  const kotlin = resolve(checkout, 'kotlin/RemoteJson.kt');
  const original = readFileSync(kotlin, 'utf8');
  const broken = original.replace('root.opt("v") != 1', 'root.opt("v") ! = 1');
  assert.notEqual(broken, original);
  writeFileSync(kotlin, broken);
  const result = run('python3', script, '--kotlin-check', 'kotlin');
  assert.notEqual(result.status, 0);
  assert.match(result.stderr, /Generated Kotlin contract differs: RemoteJson.kt/);
});

test('remote verification catches renderer DTO drift before reporting agreement', t => {
  const files = [
    'scripts/verify-remote-contract.mjs', 'scripts/generate-remote-ui-types.mjs',
    'scripts/generate-remote-contract.py', protocol, generated, 'src/features/remote/protocol.generated.ts', 'rust-toolchain.toml',
  ];
  const { checkout, run } = fixture(t, files);
  const remote = 'android/app/src/test/resources';
  mkdirSync(resolve(checkout, remote), { recursive: true });
  const verifier = 'scripts/verify-remote-contract.mjs';
  const bundle = run(process.execPath, verifier, '--update-manifest', '--bundle', `${remote}/mediacenter-v1.json`);
  assert.equal(bundle.status, 0, bundle.stderr);
  const kotlin = 'android/app/src/main/java/com/emmah/colordock/remote/mediacenter/protocol';
  const kotlinOutput = run('python3', 'scripts/generate-remote-contract.py', '--check', '--kotlin-output', kotlin);
  assert.equal(kotlinOutput.status, 0, kotlinOutput.stderr);
  writeFileSync(resolve(checkout, 'src/features/remote/protocol.generated.ts'), 'export type Example = string;\n');
  const result = run(process.execPath, verifier, '--remote-root', 'android');
  assert.notEqual(result.status, 0);
  assert.match(result.stderr, /Renderer wire DTOs are stale/);
  assert.equal(result.stdout, '');
});

test('the UI copy check finds 50% border radius with or without comments', t => {
  const { checkout, run } = fixture(t, ['scripts/verify-ui-copy.mjs']);
  mkdirSync(resolve(checkout, 'src'));
  const path = resolve(checkout, 'src/shape.css');
  for (const value of ['50%', '/* round */ 50%']) {
    writeFileSync(path, `.shape { border-radius: ${value}; }\n`);
    const result = run(process.execPath, 'scripts/verify-ui-copy.mjs');
    assert.notEqual(result.status, 0);
    assert.match(result.stderr, /Added dot decoration/);
  }
  writeFileSync(path, '.shape { border-radius: /* slight */ 5%; }\n');
  const result = run(process.execPath, 'scripts/verify-ui-copy.mjs');
  assert.equal(result.status, 0, result.stderr);
});
