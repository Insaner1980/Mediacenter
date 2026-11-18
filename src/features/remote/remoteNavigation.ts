import {NavigationError,type FocusRegistry,type NativeAction} from './focusRegistry';
import type {Route,UiRequest,UiAck} from './protocol.generated';
export interface NavigationController {navigate(route:Route):void;back():void;settle():Promise<void>}
export interface RendererRequest {dispatchId:string;requestId:string;origin:string;ownerDeviceId:string;request:UiRequest;expiresAtMs:number}
export interface RendererResponse {dispatchId:string;rendererGeneration:string;context:ReturnType<FocusRegistry['context']>;action?:NativeAction;error?:string}
// Sequence keys contain no search text and are discarded with this renderer.
export function createRemoteNavigation(){
 const edits=new Map<string,bigint>();
 return async function handleRemoteNavigation(envelope:RendererRequest,registry:FocusRegistry,navigation:NavigationController):Promise<UiAck & {action?:NativeAction}>{
  const request=envelope.request,p=request.params,current=registry.sync();
  if(Date.now()>envelope.expiresAtMs||p.rendererGeneration!==current.rendererGeneration||p.focusRevision!==current.focusRevision||p.modalId!==current.modalId)throw new NavigationError('stale_state');
  let action:NativeAction|undefined;
  switch(request.method){
   case 'ui.navigate':if(current.modalId)throw new NavigationError('stale_state');navigation.navigate(request.params.route);break;
   case 'ui.moveFocus':registry.move(request.params.direction);break;
   case 'ui.activate':{const selected=registry.activate(request.params.focusId);if(selected?.kind==='navigate')navigation.navigate(selected.route);else action=selected;break;}
   case 'ui.back':{const result=registry.back();if(!result)navigation.back();else if(typeof result==='object')action=result;break;}
   case 'ui.search':{
    const {route,text,editSequence}=request.params,key=`${envelope.origin}/${route}`,sequence=BigInt(editSequence);
    if(sequence<=(edits.get(key)??-1n))throw new NavigationError('stale_state');
    if(Array.from(text).length>200)throw new NavigationError('invalid_request');
    registry.search(route,text);edits.set(key,sequence);
    // Bound disconnected origins; never retain text or hash typed content.
    if(edits.size>256)edits.delete(edits.keys().next().value!);
    break;
   }
  }
  await navigation.settle();
  const updated=registry.sync();
  return {requestId:envelope.requestId,rendererGeneration:updated.rendererGeneration!,focusRevision:updated.focusRevision!,action};
 };
}
