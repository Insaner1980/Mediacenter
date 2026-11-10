import {TargetRegistry} from './target-registry.mjs';
const registry=new TargetRegistry();
chrome.debugger.onDetach.addListener(()=>registry.invalidate());
chrome.tabs.onRemoved.addListener(id=>{if(registry.target?.tabId===id) registry.invalidate();});
chrome.tabs.onReplaced.addListener((_,old)=>{if(registry.target?.tabId===old) registry.invalidate();});
chrome.tabs.onUpdated.addListener((id,change)=>{
 if(registry.target?.tabId===id && change.status==='loading') registry.invalidate();
});
chrome.runtime.onMessage.addListener((message,sender,reply)=>{
 if(sender.id!==chrome.runtime.id) return false;
 (async()=>{
  if(message.type==='enroll') {
   const profile=crypto.randomUUID(); await chrome.storage.local.set({profile}); registry.enroll(profile);
   return {enrolled:true,profile};
  }
  if(message.type==='probe') return {admitted:true, extensionId:chrome.runtime.id};
  throw Error('unsupported');
 })().then(reply,()=>reply({error:'bridge_unavailable'}));
 return true;
});
