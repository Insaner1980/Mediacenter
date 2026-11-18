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
type Layout='list'|'row'|'grid';
interface Entry {element:HTMLElement;id:string;group:string;section:string;layout:Layout}
function entry(element:HTMLElement):Entry {
 const group=element.dataset.remoteGroup??'content',container=element.closest<HTMLElement>('[data-remote-section]');
 const declared=container?.dataset.remoteLayout;
 const layout:Layout=declared==='list'||declared==='row'||declared==='grid'?declared:group==='tabs'?'row':group==='modal'?'grid':'list';
 return {element,id:element.dataset.remoteId!,group,section:container?.dataset.remoteSection??group,layout};
}
function sameSection(a:Entry,b:Entry){return a.group===b.group&&a.section===b.section;}
function sections(entries:Entry[]){return entries.filter((value,index)=>entries.findIndex(other=>sameSection(value,other))===index);}
function measured(entries:Entry[]){return entries.map(value=>({value,rect:value.element.getBoundingClientRect()}));}
function gridRows(entries:Entry[]){
 const rows:ReturnType<typeof measured>[]=[];
 for(const item of measured(entries).sort((a,b)=>a.rect.top-b.rect.top||a.rect.left-b.rect.left)){
  // Grid columns share a row top; variable button widths/heights do not change it.
  const row=rows.at(-1);if(row&&Math.abs(row[0].rect.top-item.rect.top)<=1)row.push(item);else rows.push([item]);
 }
 for(const row of rows)row.sort((a,b)=>a.rect.left-b.rect.left);
 return rows;
}
function closestColumn(entries:Entry[],anchor:Entry){
 const rect=anchor.element.getBoundingClientRect(),x=rect.left+rect.width/2;
 return measured(entries).sort((a,b)=>Math.abs(a.rect.left+a.rect.width/2-x)-Math.abs(b.rect.left+b.rect.width/2-x))[0]?.value;
}
function boundary(entries:Entry[],forward:boolean,anchor?:Entry){
 if(!entries.length)return;
 if(entries[0].layout==='grid'&&entries.every(item=>item.element.getBoundingClientRect().width>0)){
  const rows=gridRows(entries),row=forward?rows[0]:rows.at(-1)!;
  return anchor?closestColumn(row.map(item=>item.value),anchor):row[0]?.value;
 }
 return forward?entries[0]:entries.at(-1);
}
function withinSection(peers:Entry[],current:Entry,direction:Direction){
 const horizontal=direction==='left'||direction==='right',forward=direction==='right'||direction==='down';
 if(current.layout==='list')return horizontal?undefined:peers[peers.indexOf(current)+(forward?1:-1)];
 if(current.layout==='row'){
  if(!horizontal)return;
  const ordered=measured(peers).sort((a,b)=>a.rect.left-b.rect.left).map(item=>item.value);
  return ordered[ordered.indexOf(current)+(forward?1:-1)];
 }
 if(!peers.every(item=>{const rect=item.element.getBoundingClientRect();return rect.width>0&&rect.height>0;}))return peers[peers.indexOf(current)+(forward?1:-1)];
 const rows=gridRows(peers),rowIndex=rows.findIndex(row=>row.some(item=>item.value===current)),row=rows[rowIndex];
 if(horizontal)return row[row.findIndex(item=>item.value===current)+(forward?1:-1)]?.value;
 const adjacent=rows[rowIndex+(forward?1:-1)];
 return adjacent?closestColumn(adjacent.map(item=>item.value),current):undefined;
}
function sectionNeighbor(entries:Entry[],current:Entry,forward:boolean){
 const ordered=sections(entries.filter(item=>item.group===current.group)),index=ordered.findIndex(item=>sameSection(item,current));
 const next=ordered[index+(forward?1:-1)];
 return next?boundary(entries.filter(item=>sameSection(item,next)),forward,current):undefined;
}
function recover(previous:Entry[],next:Entry[],id:string,sameScope:boolean){
 const current=sameScope?previous.find(item=>item.id===id):undefined;
 if(current){
  const peers=previous.filter(item=>sameSection(item,current)),index=peers.findIndex(item=>item.id===id);
  const survivors=next.filter(item=>sameSection(item,current));
  for(const old of [...peers.slice(index+1),...peers.slice(0,index).reverse()]){const survivor=survivors.find(item=>item.id===old.id);if(survivor)return survivor;}
  if(survivors.length)return survivors[0];
  const oldSections=sections(previous.filter(item=>item.group===current.group)),sectionIndex=oldSections.findIndex(item=>sameSection(item,current));
  for(const [candidates,forward] of [[oldSections.slice(sectionIndex+1),true],[oldSections.slice(0,sectionIndex).reverse(),false]] as const){
   for(const section of candidates){const target=boundary(next.filter(item=>sameSection(item,section)),forward);if(target)return target;}
  }
  const sameGroup=next.find(item=>item.group===current.group);if(sameGroup)return sameGroup;
 }
 return next.find(item=>item.group==='sidebar'&&item.element.getAttribute('aria-current')==='page')??next[0];
}
export function createFocusRegistry(root:Document=document){
 let revision=0n,signature='',generation='0',route:Route='home',items:HTMLElement[]=[],order:Entry[]=[],focusId:string|undefined,previousScope='';
 function sync(){
  const modal=topmostDialog();
  const scope=modal??root;
  const next=Array.from(scope.querySelectorAll<HTMLElement>('[data-remote-id]')).filter(visible);
  const nextOrder=next.map(entry),scopeId=JSON.stringify([route,modal?.dataset.remoteModal]);
  const nextSignature=JSON.stringify([scopeId,nextOrder.map(e=>[e.section,e.layout]),next.map(e=>[e.dataset.remoteId,e.dataset.remoteMedia,e.dataset.remoteKind,e.dataset.remoteResume,e.dataset.remoteConfirm,e.dataset.remoteAccept,e.dataset.remoteRoute,e.textContent])]);
  if(signature!==nextSignature){signature=nextSignature;revision++;}
  const active=(root.activeElement as HTMLElement|null)?.dataset.remoteId;
  if(active&&next.some(e=>e.dataset.remoteId===active)){if(focusId!==active){focusId=active;revision++;}}
  else if(focusId&&!next.some(e=>e.dataset.remoteId===focusId)){
   const target=recover(order,nextOrder,focusId,scopeId===previousScope);
   focusId=target?.id;
   if(target)focusElement(target.element);
   revision++;
  }
  items=next;order=nextOrder;previousScope=scopeId;
  return context();
 }
 function context():UiTopic{return {revision:'0',available:true,rendererGeneration:generation,route,modalId:topmostDialog()?.dataset.remoteModal,focusId,focusRevision:String(revision)};}
 function focusElement(element:HTMLElement){root.querySelectorAll('[data-remote-focused]').forEach(e=>e.removeAttribute('data-remote-focused'));element.dataset.remoteFocused='true';element.focus();element.scrollIntoView?.({block:'nearest',inline:'nearest'});}
 function focus(element:HTMLElement|undefined){if(element){focusId=element.dataset.remoteId;focusElement(element);revision++;}return sync();}
 return {
  sync,context,
  configure(nextGeneration:string,nextRoute:Route){generation=nextGeneration;route=nextRoute;return sync();},
  move(direction:Direction){
   sync();const current=order.find(item=>item.id===focusId);
   if(!current)return focus(items[0]);
   const {group}=current,peers=order.filter(item=>sameSection(item,current));
   let target=withinSection(peers,current,direction);
   // Content sections are explicitly ordered: search → items → pagination.
   if(!target&&group==='content'&&(direction==='up'||direction==='down'))target=sectionNeighbor(order,current,direction==='down');
   if(!target&&group==='tabs'&&direction==='down')target=order.find(item=>item.group==='content');
   // Defined section transitions never cross the topmost modal boundary.
   if(!target&&!topmostDialog()&&direction==='up'&&group==='content')target=order.find(item=>item.group==='tabs'&&item.element.getAttribute('aria-selected')==='true')??order.find(item=>item.group==='tabs');
   if(!target&&!topmostDialog()&&direction==='right'&&group==='sidebar')target=order.find(item=>item.group!=='sidebar');
   if(!target&&!topmostDialog()&&direction==='left'&&group!=='sidebar')target=order.find(item=>item.group==='sidebar'&&item.element.getAttribute('aria-current')==='page')??order.find(item=>item.group==='sidebar');
   return focus(target?.element);
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
