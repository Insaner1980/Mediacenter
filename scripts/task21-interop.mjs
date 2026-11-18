// Offline Task21 exporter/installed-resource guard. Never writes the protected Remote tree.
import {execFileSync} from 'node:child_process';
import {createHash} from 'node:crypto';
import {readFileSync,writeFileSync,mkdirSync,existsSync,mkdtempSync,copyFileSync} from 'node:fs';
import {resolve,dirname} from 'node:path';
import {tmpdir} from 'node:os';
import {fileURLToPath} from 'node:url';
const root=resolve(dirname(fileURLToPath(import.meta.url)),'..');
const plan=resolve(root,'.superpowers/sdd/2026-11-09-remote-mediacenter-sofa-control');
const stage=resolve(plan,'android-task21');
const resource='app/src/test/resources/mediacenter-rust-interop-v1.json';
const consumer='app/src/test/java/com/emmah/colordock/remote/mediacenter/RustInteropTest.kt';
const kotlin='app/src/main/java/com/emmah/colordock/remote/mediacenter/';
const remote=resolve(root,'../Remote');
const fresh=resolve(plan,'task-21-fresh-corpus.json');
const sha=bytes=>createHash('sha256').update(bytes).digest('hex');
const inputs=[
 'scripts/task21-interop.mjs','scripts/task21-interop.test.mjs','protocol/remote/v1/schema.json','protocol/remote/v1/fixtures.json','protocol/remote/v1/manifest.json',
 ...['interop_tests','mod','protocol','protocol_json','protocol_methods','adapters','dispatch','operations','store','server','state','event_hub'].map(name=>`src-tauri/src/remote/${name}.rs`),
];
const remoteInputs=[`${kotlin}protocol/RemoteJson.kt`,`${kotlin}protocol/Messages.kt`,`${kotlin}protocol/StrictJson.kt`,`${kotlin}RemoteStateReducer.kt`];
function hashes(){return Object.fromEntries([
 ...inputs.map(path=>[`MC/${path}`,sha(readFileSync(resolve(root,path)))]),
 ...remoteInputs.map(path=>[`Remote/${path}`,sha(readFileSync(resolve(remote,path)))]),
 [`Remote/${consumer}`,sha(readFileSync(resolve(stage,consumer)))],
]);}
function invariant(value,message){if(!value)throw Error(message);}
function validate(corpus){
 invariant(corpus.format==='mediacenter-rust-interop-v1','Wrong corpus format');
 const fixtures=JSON.parse(readFileSync(resolve(root,'protocol/remote/v1/fixtures.json')));
 const expected=fixtures.requests.map(v=>v.method).sort();
 for(const [name,entries] of [['requests',corpus.requests],['replies',corpus.replies.slice(0,88)]]){
  invariant(entries.length===88&&JSON.stringify(entries.map(v=>v.method).sort())===JSON.stringify(expected),`${name}: missing/duplicate method shapes`);
 }
 invariant(corpus.snapshots.length===2&&corpus.invalidRequests.length>=44&&corpus.invalidReplies.length===2,'Vacuous boundary corpus');
 invariant(corpus.dispatcher.effectCount===1,'Native effect count changed');
 invariant(JSON.stringify(corpus.provenance?.sourceSha256)===JSON.stringify(hashes()),'Exporter/parser/consumer source hashes changed: prepare and guarded install again');
}
const action=process.argv[2];
invariant(['prepare','verify'].includes(action),'Usage: node scripts/task21-interop.mjs prepare|verify');
if(action==='prepare'){
 if(!existsSync(resolve(stage,consumer))){mkdirSync(dirname(resolve(stage,consumer)),{recursive:true});copyFileSync(resolve(remote,consumer),resolve(stage,consumer));console.log(`Task21 bootstrapped staged consumer from installed paired source: ${sha(readFileSync(resolve(stage,consumer)))}`);}
 console.log(`Task21 consumer input: ${resolve(stage,consumer)}; SHA256 ${sha(readFileSync(resolve(stage,consumer)))}`);
 const raw=resolve(mkdtempSync(resolve(tmpdir(),'mediacenter-task21-')),'rust-export.json');
 execFileSync('cargo',['test','--manifest-path',resolve(root,'src-tauri/Cargo.toml'),'--lib','remote::interop_tests::task21_export_rust_client_corpus','--','--exact','--nocapture'],{cwd:root,env:{...process.env,MEDIACENTER_TASK21_CORPUS_OUT:raw},stdio:'inherit'});
 const corpus=JSON.parse(readFileSync(raw));
 corpus.provenance={baseMediacenter:'5dd0bee09028ab68d258eecffd79015e3edda2c1',baseRemote:'899edad359f06262061e2345200ef49e48f04657',sourceSha256:hashes()};
 validate(corpus);
 const bytes=JSON.stringify(corpus,null,2)+'\n';
 writeFileSync(fresh,bytes);
 mkdirSync(dirname(resolve(stage,resource)),{recursive:true});writeFileSync(resolve(stage,resource),bytes);
 const files=[consumer,resource].map(path=>({path,baseSha256:existsSync(resolve(remote,path))?sha(readFileSync(resolve(remote,path))):null,sha256:sha(readFileSync(resolve(stage,path)))}));
 writeFileSync(resolve(stage,'manifest.json'),JSON.stringify({files},null,2)+'\n');
 console.log(`Task21 freshly exported and staged: ${sha(bytes)}; guarded parent install required.`);
}else{
 const bytes=readFileSync(fresh);const corpus=JSON.parse(bytes);validate(corpus);
 invariant(bytes.equals(readFileSync(resolve(stage,resource))),'Staged corpus differs from fresh exporter output');
 invariant(bytes.equals(readFileSync(resolve(remote,resource))),'Installed corpus differs from fresh exporter output');
 invariant(readFileSync(resolve(stage,consumer)).equals(readFileSync(resolve(remote,consumer))),'Installed consumer differs from frozen stage');
 console.log(`Task21 fresh Rust → installed Kotlin resource verified: ${sha(bytes)}; 88 methods; native effect count 1.`);
}
