// Disposable actual-Chrome admission/retention probe. No TCP debugging port.
import {spawn} from 'node:child_process';
import {mkdir,readFile,writeFile} from 'node:fs/promises';
import path from 'node:path';
const root=path.resolve('browser-extension');
const isolated=path.resolve('.tools/browser-feasibility');
const id=(await readFile(path.join(root,'extension-id.txt'),'utf8')).trim();
await mkdir(isolated,{recursive:true,mode:0o700});
const supported=process.argv.includes('--supported-ui');
const installed=process.argv.includes('--installed');
if(supported && installed) throw Error('choose one installation mode');
const results={installation:installed?'already-installed':supported?'supported-ui-drag-drop':'experimental-cdp',chrome:'/opt/google/chrome/google-chrome',extensionId:id};
async function run(load) {
 const args=[`--user-data-dir=${isolated}/profile`,'--no-first-run','--no-default-browser-check','--remote-debugging-pipe'];
 if(load && !supported && !installed) args.push('--enable-unsafe-extension-debugging');
 const child=spawn(results.chrome,args,{env:{...process.env,XDG_CONFIG_HOME:`${isolated}/config`},stdio:['ignore','ignore','pipe','pipe','pipe']});
 const pending=new Map(); let seq=0,buffer=''; let stderr='';
 child.stderr.on('data',b=>{stderr+=b.toString();});
 child.stdio[4].on('data',b=>{buffer+=b.toString();let i;while((i=buffer.indexOf('\0'))>=0){const text=buffer.slice(0,i);buffer=buffer.slice(i+1);if(!text)continue;const m=JSON.parse(text);if(m.id){const p=pending.get(m.id);pending.delete(m.id);if(p){clearTimeout(p.timer);m.error?p.reject(Error(JSON.stringify(m.error))):p.resolve(m.result);}}}});
 const call=(method,params={},sessionId)=>new Promise((resolve,reject)=>{const n=++seq;const timer=setTimeout(()=>{pending.delete(n);reject(Error(`timeout ${method}`));},12000);pending.set(n,{resolve,reject,timer});child.stdio[3].write(JSON.stringify({id:n,method,params,...(sessionId?{sessionId}:{})})+'\0');});
 try {
  const version=await call('Browser.getVersion');
  if(load && !supported && !installed) await call('Extensions.loadUnpacked',{path:root});
  if(load && supported) {
   const {targetId:uiTarget}=await call('Target.createTarget',{url:'chrome://extensions/'});
   const {sessionId:uiSession}=await call('Target.attachToTarget',{targetId:uiTarget,flatten:true});
   const evaluate=async expression=>{
    const value=await call('Runtime.evaluate',{expression,returnByValue:true},uiSession);
    if(value.exceptionDetails) throw Error('extensions UI evaluation failed');
    return value.result.value;
   };
   // Read rendered UI only; do not call developerPrivate or modify Chrome preferences.
   const toggleExpression=`(() => {
    const manager=document.querySelector('extensions-manager');
    const toolbar=manager?.shadowRoot?.querySelector('extensions-toolbar');
    const toggle=toolbar?.shadowRoot?.querySelector('#devMode');
    if(!toggle) return null;
    const r=toggle.getBoundingClientRect(); return {checked:toggle.checked,x:r.x+r.width/2,y:r.y+r.height/2};
   })()`;
   let toggle;
   for(let i=0;i<20;i++){toggle=await evaluate(toggleExpression);if(toggle)break;await new Promise(r=>setTimeout(r,150));}
   if(!toggle) throw Error('supported installation requires desktop directory chooser: developer-mode UI unavailable');
   if(!toggle.checked) {
    await call('Input.dispatchMouseEvent',{type:'mousePressed',x:toggle.x,y:toggle.y,button:'left',clickCount:1},uiSession);
    await call('Input.dispatchMouseEvent',{type:'mouseReleased',x:toggle.x,y:toggle.y,button:'left',clickCount:1},uiSession);
   }
   if(!(await evaluate(toggleExpression))?.checked) throw Error('developer-mode toggle not enabled');
   const viewport=await evaluate('({width:innerWidth,height:innerHeight})');
   const data={items:[],files:[root],dragOperationsMask:1};
   for(const type of ['dragEnter','dragOver','drop']) await call('Input.dispatchDragEvent',{type,x:viewport.width/2,y:viewport.height/2,data},uiSession);
   await new Promise(r=>setTimeout(r,1500));
   const shot=await call('Page.captureScreenshot',{format:'png'},uiSession);
   await writeFile(`${isolated}/supported-install-ui.png`,Buffer.from(shot.data,'base64'));
  }
  const {targetId}=await call('Target.createTarget',{url:`chrome-extension://${id}/enroll.html`});
  const {sessionId}=await call('Target.attachToTarget',{targetId,flatten:true});
  let result;
  for(let i=0;i<20;i++) {
   result=await call('Runtime.evaluate',{expression:'typeof window.probeRun === "function"',returnByValue:true},sessionId);
   if(result.result.value) break;
   await new Promise(r=>setTimeout(r,150));
  }
  if(!result.result.value) throw Error('extension page unavailable');
  const enrollment=await call('Runtime.evaluate',{expression:load?'window.probeEnrollment()':'chrome.storage.local.get("profile")',awaitPromise:true,returnByValue:true},sessionId);
  const probe=await call('Runtime.evaluate',{expression:'window.probeRun()',awaitPromise:true,returnByValue:true},sessionId);
  if(enrollment.exceptionDetails || probe.exceptionDetails) throw Error('extension evaluation exception');
  const enrolled=enrollment.result.value; const admitted=probe.result.value;
  if(admitted?.admitted!==true || admitted.extensionId!==id) throw Error('extension admission failed');
  if(load && (enrolled?.enrolled!==true || typeof enrolled.profile!=='string' || !enrolled.profile)) throw Error('enrollment failed');
  if(!load && enrolled?.profile!==results.admission.enrollment.profile) throw Error('profile enrollment not retained');
  return {version:version.product,args,enrollment:enrolled,probe:admitted};
 } finally {
  try{await call('Browser.close');}catch{}
  await new Promise(resolve=>{if(child.exitCode!==null)return resolve();child.once('exit',resolve);setTimeout(()=>{child.kill('SIGTERM');resolve();},5000).unref();});
  // Only diagnostic metadata, never webpage or entered text.
  await writeFile(`${isolated}/${installed?'installed-':supported?'supported-':''}${load?'admission':'retention'}-stderr.log`,stderr);
 }
}
try{results.admission=await run(true);results.retention=await run(false);results.passed=true;}catch(error){results.passed=false;results.error=error.message; if(supported) results.handoff={command:`XDG_CONFIG_HOME=${isolated}/config /opt/google/chrome/google-chrome --user-data-dir=${isolated}/profile --no-first-run --no-default-browser-check chrome://extensions/`,instruction:'Enable Developer mode, choose Load unpacked, and select the exact extension folder.',folder:root};}
await writeFile(`${isolated}/${installed?'installed-retention':supported?'supported-install':'admission'}-result.json`,JSON.stringify(results,null,2)+'\n');
console.log(JSON.stringify(results,null,2));
if(!results.passed) process.exitCode=1;
