import {topmostDialog,closeRemoteDialog} from '../../ui/Dialog';
import type {Direction,Route,UiTopic} from './protocol.generated';
export type NativeAction = {kind:'media';mediaKind:'video'|'music'|'photo';itemId:string;resume:boolean} | {kind:'confirmation';confirmationId:string;confirm:boolean};
export type FocusAction=NativeAction|{kind:'navigate';route:Route};
export class NavigationError extends Error {constructor(public code:'stale_state'|'forbidden'|'unavailable'|'invalid_request'){super(code);}}
function visible(element:HTMLElement){
 if(element.matches(':disabled')||element.closest('[hidden], [inert], details:not([open])'))return false;
 for(let parent:HTMLElement|null=element;parent;parent=parent.parentElement){const style=getComputedStyle(parent);if(parent.inert||style.display==='none'||style.visibility==='hidden')return false;}
 return true;
}
export function createFocusRegistry(root:Document=document){
 let revision=0n,signature='',generation='0',route:Route='home',items:HTMLElement[]=[],focusId:string|undefined,previousIndex=0;
 function sync(){
  const modal=topmostDialog();
  const scope=modal??root;
  const next=Array.from(scope.querySelectorAll<HTMLElement>('[data-remote-id]')).filter(visible);
  const nextSignature=JSON.stringify([route,modal?.dataset.remoteModal,next.map(e=>[e.dataset.remoteId,e.dataset.remoteMedia,e.dataset.remoteKind,e.dataset.remoteResume,e.dataset.remoteConfirm,e.dataset.remoteAccept,e.dataset.remoteRoute,e.textContent])]);
  if(signature!==nextSignature){signature=nextSignature;revision++;}
  const active=(root.activeElement as HTMLElement|null)?.dataset.remoteId;
  if(active&&next.some(e=>e.dataset.remoteId===active)){if(focusId!==active){focusId=active;revision++;}}
  else if(focusId&&!next.some(e=>e.dataset.remoteId===focusId)){
   focusId=next[Math.min(previousIndex,Math.max(0,next.length-1))]?.dataset.remoteId;
   next.find(e=>e.dataset.remoteId===focusId)?.focus();
   revision++;
  }
  items=next;
  if(focusId)previousIndex=Math.max(0,items.findIndex(e=>e.dataset.remoteId===focusId));
  return context();
 }
 function context():UiTopic{return {revision:'0',available:true,rendererGeneration:generation,route,modalId:topmostDialog()?.dataset.remoteModal,focusId,focusRevision:String(revision)};}
 function focus(element:HTMLElement|undefined){if(element){root.querySelectorAll('[data-remote-focused]').forEach(e=>e.removeAttribute('data-remote-focused'));element.dataset.remoteFocused='true';element.focus();element.scrollIntoView?.({block:'nearest',inline:'nearest'});focusId=element.dataset.remoteId;revision++;}return sync();}
 return {
  sync,context,
  configure(nextGeneration:string,nextRoute:Route){generation=nextGeneration;route=nextRoute;return sync();},
  move(direction:Direction){
   sync();const current=items.find(e=>e.dataset.remoteId===focusId);
   if(!current)return focus(items[0]);
   const group=current.dataset.remoteGroup;
   const peers=items.filter(e=>e.dataset.remoteGroup===group);
   const index=peers.indexOf(current);let target:HTMLElement|undefined;
   const a=current.getBoundingClientRect();
   const horizontal=direction==='left'||direction==='right',positive=direction==='right'||direction==='down';
   // Spatial grid movement uses measured centers, never an assumed column count.
   if(group==='tabs'&&!horizontal){target=direction==='down'?items.find(e=>e.dataset.remoteGroup==='content'):undefined;}
   else if(group!=='sidebar'&&a.width>0){
    const candidates=peers.filter(e=>e!==current).map(e=>{const b=e.getBoundingClientRect();const dx=b.left+b.width/2-a.left-a.width/2,dy=b.top+b.height/2-a.top-a.height/2;return {e,along:horizontal?dx:dy,cross:horizontal?dy:dx};}).filter(p=>positive?p.along>1:p.along< -1).sort((x,y)=>(Math.abs(x.cross)*1000+Math.abs(x.along))-(Math.abs(y.cross)*1000+Math.abs(y.along)));
    target=candidates[0]?.e;
   }else if(!horizontal||group!=='sidebar')target=peers[index+(positive?1:-1)];
   // Defined section transition: sidebar → content; content's left edge → sidebar.
   if(!target&&!topmostDialog()&&direction==='up'&&group==='content')target=items.find(e=>e.dataset.remoteGroup==='tabs'&&e.getAttribute('aria-selected')==='true')??items.find(e=>e.dataset.remoteGroup==='tabs');
   if(!target&&!topmostDialog()&&direction==='right'&&group==='sidebar')target=items.find(e=>e.dataset.remoteGroup!=='sidebar');
   if(!target&&!topmostDialog()&&direction==='left'&&group!=='sidebar')target=items.find(e=>e.dataset.remoteGroup==='sidebar'&&e.getAttribute('aria-current')==='page')??items.find(e=>e.dataset.remoteGroup==='sidebar');
   return focus(target);
  },
  activate(id:string):FocusAction|undefined{
   sync();if(id!==focusId)throw new NavigationError('stale_state');
   const element=items.find(e=>e.dataset.remoteId===id);if(!element)throw new NavigationError('stale_state');
   const route=element.dataset.remoteRoute;
   if(route&&['home','videos','music','photos','services','settings'].includes(route))return {kind:'navigate',route:route as Route};
   if(element.dataset.remoteMedia)return {kind:'media',mediaKind:element.dataset.remoteKind as 'video'|'music'|'photo',itemId:element.dataset.remoteMedia,resume:element.dataset.remoteResume==='true'};
   if(element.dataset.remoteConfirm)return {kind:'confirmation',confirmationId:element.dataset.remoteConfirm,confirm:element.dataset.remoteAccept==='true'};
   if(element.dataset.remoteSearch){element.focus();return;}
   if(element.dataset.remoteLocal==='navigation'||element.dataset.remoteLocal==='dismiss'){element.click();return;}
   throw new NavigationError('forbidden');
  },
  back():boolean|NativeAction{const id=topmostDialog()?.dataset.remoteConfirmation;if(id)return {kind:'confirmation',confirmationId:id,confirm:false};const result=closeRemoteDialog();if(result==='blocked')throw new NavigationError('forbidden');return result==='closed';},
  search(target:Route,text:string){
   sync();if(topmostDialog()||target!==route)throw new NavigationError('stale_state');
   const input=items.find(e=>e.dataset.remoteSearch===target);
   if(!input)throw new NavigationError('unavailable');
   if(input.dataset.remoteComposing==='true')throw new NavigationError('stale_state');
   input.dispatchEvent(new CustomEvent('remote-search',{detail:text}));
  },
 };
}
export type FocusRegistry=ReturnType<typeof createFocusRegistry>;
