// Freshness guard regression tests use only isolated copies of sanitized test data/source.
import {test} from 'node:test';
import {Buffer} from 'node:buffer';
import assert from 'node:assert/strict';
import {mkdtempSync,mkdirSync,readFileSync,writeFileSync,copyFileSync,rmSync} from 'node:fs';
import {tmpdir} from 'node:os';
import {resolve,dirname} from 'node:path';
import {fileURLToPath} from 'node:url';
import {spawnSync} from 'node:child_process';
const root=resolve(dirname(fileURLToPath(import.meta.url)),'..');
const relativePlan='.superpowers/sdd/2026-11-09-remote-mediacenter-sofa-control';
const plan=resolve(root,relativePlan);
const consumer='app/src/test/java/com/emmah/colordock/remote/mediacenter/RustInteropTest.kt';
const resource='app/src/test/resources/mediacenter-rust-interop-v1.json';
function isolated(){
 const temporary=mkdtempSync(resolve(tmpdir(),'task21-guard-'));
 const mc=resolve(temporary,'MC'),remote=resolve(temporary,'Remote');
 const prepared=readFileSync(resolve(plan,'task-21-fresh-corpus.json'));
 const provenance=JSON.parse(prepared).provenance;
 for(const name of Object.keys(provenance.sourceSha256)){
  const [repo,...parts]=name.split('/');const path=parts.join('/');
  const target=resolve(temporary,repo,path);mkdirSync(dirname(target),{recursive:true});
  const source=repo==='MC'?resolve(root,path):path===consumer?resolve(plan,'android-task21',consumer):resolve(root,'../Remote',path);
  copyFileSync(source,target);
 }
 const targetPlan=resolve(mc,relativePlan);
 for(const path of [resolve(targetPlan,'task-21-fresh-corpus.json'),resolve(targetPlan,'android-task21',resource),resolve(remote,resource)]){mkdirSync(dirname(path),{recursive:true});writeFileSync(path,prepared);}
 const staged=resolve(targetPlan,'android-task21',consumer);mkdirSync(dirname(staged),{recursive:true});copyFileSync(resolve(remote,consumer),staged);
 return {temporary,mc,remote,targetPlan};
}
function verify(fixture){return spawnSync(process.execPath,[resolve(fixture.mc,'scripts/task21-interop.mjs'),'verify'],{encoding:'utf8'});}
test('prepared bytes verify; installed tampering fails with no protected-tree writes',()=>{
 const fixture=isolated();
 try{
  const good=verify(fixture);assert.equal(good.status,0,good.stderr);
  const path=resolve(fixture.remote,resource),corpus=JSON.parse(readFileSync(path));
  const hello=JSON.parse(corpus.hello);hello.snapshotSequence=12;corpus.hello=JSON.stringify(hello);writeFileSync(path,JSON.stringify(corpus));
  const bad=verify(fixture);assert.notEqual(bad.status,0);assert.match(bad.stderr,/Installed corpus differs from fresh exporter output/);
 }finally{rmSync(fixture.temporary,{recursive:true,force:true});}
});
test('changed serializer source and missing prepared output fail closed',()=>{
 const fixture=isolated();
 try{
  const path=resolve(fixture.mc,'src-tauri/src/remote/protocol.rs');const original=readFileSync(path);writeFileSync(path,Buffer.concat([original,Buffer.from('\n// isolated drift probe\n')]));
  const stale=verify(fixture);assert.notEqual(stale.status,0);assert.match(stale.stderr,/source hashes changed/);
  writeFileSync(path,original);rmSync(resolve(fixture.targetPlan,'task-21-fresh-corpus.json'));
  const absent=verify(fixture);assert.notEqual(absent.status,0);assert.match(absent.stderr,/ENOENT/);
 }finally{rmSync(fixture.temporary,{recursive:true,force:true});}
});
