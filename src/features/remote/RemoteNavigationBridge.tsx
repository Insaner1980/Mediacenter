import {remoteRouteFor as wireRoute} from '../../app/navigation';
import {useEffect,useRef,useState} from 'react';
import {flushSync} from 'react-dom';
import {invoke,isTauri} from '@tauri-apps/api/core';
import {listen} from '@tauri-apps/api/event';
import type {AppRoute} from '../../app/navigation';
import {Dialog} from '../../ui/Dialog';
import {createFocusRegistry,NavigationError} from './focusRegistry';
import {createRemoteNavigation,type RendererRequest,type RendererResponse} from './remoteNavigation';
import type {ConfirmationPreview,Route} from './protocol.generated';
const appRoute:Record<Route,AppRoute>={home:'home',videos:'video',music:'audio',photos:'photo',settings:'settings',services:'streaming'};
export function RemoteNavigationBridge({route,navigate,back}:{route:AppRoute;navigate:(route:AppRoute)=>void;back?:()=>void}){
 const latest=useRef({route,navigate,back});latest.current={route,navigate,back};
 const [previews,setPreviews]=useState<ConfirmationPreview[]>([]),[error,setError]=useState(''),[busy,setBusy]=useState(false);
 const refresh=useRef<()=>Promise<void>>(async()=>{});
 useEffect(()=>{
  if(!isTauri())return;
  let live=true,generation='',chain=Promise.resolve(),lastContext='',confirmationRead=0,presentationRevision=0n;const registry=createFocusRegistry(),handle=createRemoteNavigation();const cleanups:(()=>void)[]=[];
  async function publish(){if(!live||!generation)return;registry.configure(generation,wireRoute[latest.current.route]);const context=registry.sync(),serialized=JSON.stringify(context);if(serialized===lastContext)return;lastContext=serialized;await invoke('update_remote_renderer',{generation,context});}
  refresh.current=async()=>{const read=++confirmationRead;try{const value=await invoke<ConfirmationPreview[]>('get_renderer_confirmations');if(live&&read===confirmationRead)setPreviews(value);}catch{/* Remote setup can be disabled/unavailable while local media works. */}};
  async function start(){
   cleanups.push(await listen<RendererRequest>('remote-ui-request',event=>{
    chain=chain.then(async()=>{
     if(!live||!generation)return;
     registry.configure(generation,wireRoute[latest.current.route]);
     const response:RendererResponse={dispatchId:event.payload.dispatchId,rendererGeneration:generation,context:registry.sync()};
     try{
      // Explicit actions only; synchronous React commits precede the acknowledgement.
      let pending!:ReturnType<typeof handle>;flushSync(()=>{pending=handle(event.payload,registry,{navigate:r=>latest.current.navigate(appRoute[r]),back:()=>latest.current.back?latest.current.back():latest.current.navigate('home'),settle:async()=>{await Promise.resolve();registry.configure(generation,wireRoute[latest.current.route]);}});});const result=await pending;
      response.action=result.action;
     }catch(reason){response.error=reason instanceof NavigationError?reason.code:'unavailable';}
     response.context=registry.sync();
     await invoke('acknowledge_remote_renderer',{response}).catch(()=>{});
     await publish().catch(()=>{});
    });
   }));
   cleanups.push(await listen<{dispatchId:string;requestId:string;rendererGeneration:string;presentationRevision:string;previews:ConfirmationPreview[];expiresAtMs:number}>('remote-ui-confirmation',event=>{
    chain=chain.then(async()=>{
     if(!live||!generation)return;
     const request=event.payload;
     const response:RendererResponse={dispatchId:request.dispatchId,rendererGeneration:generation,context:registry.sync()};
     if(request.rendererGeneration!==generation||request.expiresAtMs<Date.now()||BigInt(request.presentationRevision)<=presentationRevision)response.error='stale_state';
     else {presentationRevision=BigInt(request.presentationRevision);confirmationRead++;flushSync(()=>setPreviews(request.previews));await Promise.resolve();response.context=registry.sync();}
     await invoke('acknowledge_remote_renderer',{response}).catch(()=>{});
     await publish().catch(()=>{});
    });
   }));
   if(!live)return;
   generation=await invoke<string>('register_remote_renderer');if(!live||typeof generation!=='string')return;
   await publish();await refresh.current();
   const observer=new MutationObserver(()=>{void publish().catch(()=>{});});observer.observe(document.body,{subtree:true,childList:true,attributes:true,attributeFilter:['disabled','open','inert','data-remote-id','aria-current']});cleanups.push(()=>observer.disconnect());
   const focused=()=>{void publish().catch(()=>{});};document.addEventListener('focusin',focused);cleanups.push(()=>document.removeEventListener('focusin',focused));
   const heartbeat=setInterval(()=>{if(live)void invoke('update_remote_renderer',{generation,context:registry.configure(generation,wireRoute[latest.current.route])}).catch(()=>{});},2000);cleanups.push(()=>clearInterval(heartbeat));
  }
  void start().catch(()=>{}).finally(()=>{if(!live)cleanups.forEach(stop=>stop());});
  return()=>{live=false;cleanups.forEach(stop=>stop());};
 },[]);
 const preview=previews[0];
 useEffect(()=>{if(!preview)return;const timer=setTimeout(()=>{void refresh.current();},preview.expiresInMs+20);return()=>clearTimeout(timer);},[preview]);
 async function resolve(confirm:boolean){if(!preview||busy)return;setBusy(true);setError('');try{await invoke('resolve_renderer_confirmation',{confirmationId:preview.confirmationId,confirm});}catch{setError('The confirmation changed. Review the current request.');}finally{await refresh.current();setBusy(false);}}
 return <Dialog open={Boolean(preview)} title="Confirm remote request" remoteConfirmation={preview?.confirmationId} busy={busy} onRequestClose={()=>void resolve(false)}>{preview&&<><p>{preview.action==='change_audio'?'Change sound output?':'Replace the current activity?'}</p><ul>{preview.affectedLabels.map((label,index)=><li key={index}>{label}</li>)}</ul>{error&&<p role="alert">{error}</p>}<div className="actions"><button className="control primary" data-remote-id={`confirm-${preview.confirmationId}`} data-remote-group="modal" data-remote-confirm={preview.confirmationId} data-remote-accept="true" disabled={busy} onClick={()=>void resolve(true)}>Confirm</button><button className="control" data-remote-id={`cancel-${preview.confirmationId}`} data-remote-group="modal" data-remote-confirm={preview.confirmationId} data-remote-accept="false" disabled={busy} onClick={()=>void resolve(false)}>Cancel</button></div></>}</Dialog>;
}
