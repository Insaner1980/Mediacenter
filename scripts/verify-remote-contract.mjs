import { execFileSync } from 'node:child_process';
import { createHash } from 'node:crypto';
import { readFile, writeFile } from 'node:fs/promises';
import { dirname, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

const root = resolve(dirname(fileURLToPath(import.meta.url)), '..');
const protocol = resolve(root, 'protocol/remote/v1');
export function canonical(value) {
  if (Array.isArray(value)) return `[${value.map(canonical).join(',')}]`;
  if (value !== null && typeof value === 'object') {
    return `{${Object.keys(value).sort().map(key => `${JSON.stringify(key)}:${canonical(value[key])}`).join(',')}}`;
  }
  return JSON.stringify(value);
}
const hash = value => createHash('sha256').update(canonical(value)).digest('hex');
const readJson = async path => JSON.parse(await readFile(path, 'utf8'));

async function main() {
  const args = process.argv.slice(2);
  const schema = await readJson(resolve(protocol, 'schema.json'));
  const fixtures = await readJson(resolve(protocol, 'fixtures.json'));
  const manifest = {
    protocolMajor: 1,
    schemaSha256: hash(schema),
    fixturesSha256: hash(fixtures),
    requestMethods: schema['x-methods'].length,
  };
  const names = schema['x-methods'].map(method => method.name).sort();
  const fixtureNames = fixtures.requests.map(request => request.method).sort();
  if (new Set(names).size !== names.length || canonical(names) !== canonical(fixtureNames)) {
    throw new Error('Every allowlisted method must have exactly one request fixture.');
  }
  const resultNames = fixtures.replies.map(entry => entry.method).sort();
  if (canonical(names) !== canonical(resultNames) || schema['x-methods'].some(method => !schema.$defs[method.result])) {
    throw new Error('Every allowlisted method must have one typed success fixture and declared result shape.');
  }
  if (args.includes('--update-manifest')) {
    await writeFile(resolve(protocol, 'manifest.json'), `${JSON.stringify(manifest, null, 2)}\n`);
    // Staged for a separately authorized write to the Android repository.
    const output = args.indexOf('--bundle');
    if (output >= 0 && args[output + 1]) {
      await writeFile(resolve(args[output + 1]), `${JSON.stringify({ manifest, schema, fixtures }, null, 2)}\n`);
    }
    console.log(`Updated v1 manifest: ${names.length} methods, ${manifest.schemaSha256}`);
    return;
  }
  const committedManifest = await readJson(resolve(protocol, 'manifest.json'));
  if (canonical(manifest) !== canonical(committedManifest)) throw new Error('Local remote protocol manifest is stale.');
  const index = args.indexOf('--remote-root');
  if (index >= 0 && !args[index + 1]) throw new Error('--remote-root requires a path.');
  const remoteRoot = index >= 0 ? resolve(args[index + 1]) : resolve(root, '../Remote');
  const remote = await readJson(resolve(remoteRoot, 'app/src/test/resources/mediacenter-v1.json'));
  if (canonical(remote.manifest) !== canonical(manifest) || hash(remote.schema) !== manifest.schemaSha256 || hash(remote.fixtures) !== manifest.fixturesSha256) {
    throw new Error('Android and desktop remote contracts differ. Copy the reviewed bundle and rerun both parser suites.');
  }
  execFileSync('python3', [resolve(root, 'scripts/generate-remote-contract.py'), '--check', '--kotlin-check', resolve(remoteRoot, 'app/src/main/java/com/emmah/colordock/remote/mediacenter/protocol')], { stdio: 'pipe' });
  console.log(`Remote v1 contracts agree: ${names.length} methods; schema ${manifest.schemaSha256}`);
}
if (process.argv[1] && resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  main().catch(error => { console.error(error.message); process.exitCode = 1; });
}
