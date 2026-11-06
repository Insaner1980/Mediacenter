import {useEffect,useId,useRef,type ReactNode,type RefObject} from 'react';
import {createPortal} from 'react-dom';
import './controls.css';
export interface DialogProps {open:boolean;title:string;onRequestClose:()=>void;children:ReactNode;footer?:ReactNode;initialFocusRef?:RefObject<HTMLElement|null>;busy?:boolean;variant?:'sheet'|'dialog'|'viewer'|'popover'}
const focusable='button:not(:disabled), input:not(:disabled), select:not(:disabled), textarea:not(:disabled), summary, a[href], [tabindex="0"]';
// Multiple modals share ownership of background inertness. Closing a parent
// and confirmation in one React commit must not restore an intermediate state.
const inertOwners=new Map<HTMLElement,{count:number;original:boolean}>();
const modalStack:HTMLDialogElement[]=[];
let rootOpener:HTMLElement|null=null;
function acquire(element:HTMLElement){const state=inertOwners.get(element);if(state)state.count++;else inertOwners.set(element,{count:1,original:element.inert});element.inert=true;}
function release(element:HTMLElement){const state=inertOwners.get(element);if(!state)return;state.count--;if(!state.count){element.inert=state.original;inertOwners.delete(element);}}
export function Dialog({open,title,onRequestClose,children,footer,initialFocusRef,busy=false,variant='dialog'}:DialogProps){
 const ref=useRef<HTMLDialogElement>(null),close=useRef(onRequestClose),blocked=useRef(busy);const id=useId();
 useEffect(()=>{close.current=onRequestClose;blocked.current=busy;},[onRequestClose,busy]);
 useEffect(()=>{
  if(!open)return;const element=ref.current!;const opener=document.activeElement as HTMLElement|null;
  if(typeof element.showModal==='function')element.showModal();else element.setAttribute('open','');
  const siblings=Array.from(document.body.children).filter(e=>e!==element) as HTMLElement[];
  if(!modalStack.length)rootOpener=opener;modalStack.push(element);siblings.forEach(acquire);
  (initialFocusRef?.current??element.querySelector<HTMLElement>(focusable)??element).focus();
  function key(event:KeyboardEvent){if(event.key==='Escape'){event.preventDefault();event.stopPropagation();if(!blocked.current)close.current();}if(event.key==='Tab'){const items=Array.from(element.querySelectorAll<HTMLElement>(focusable)).filter(e=>!e.closest('details:not([open])')||e.tagName==='SUMMARY');const first=items[0],last=items.at(-1);if(!first){event.preventDefault();element.focus();}else if(event.shiftKey&&(document.activeElement===first||document.activeElement===element)){event.preventDefault();last?.focus();}else if(!event.shiftKey&&document.activeElement===last){event.preventDefault();first.focus();}}}
  element.addEventListener('keydown',key);
  return()=>{element.removeEventListener('keydown',key);if(typeof element.close==='function')element.close();else element.removeAttribute('open');siblings.forEach(release);modalStack.splice(modalStack.indexOf(element),1);if(!modalStack.length){if(rootOpener?.isConnected)rootOpener.focus();rootOpener=null;}else if(opener?.isConnected&&!opener.closest('[inert]'))opener.focus();};
 },[open,initialFocusRef]);
 if(!open)return null;
 return createPortal(<dialog ref={ref} tabIndex={-1} className={`modal modal-${variant}`} aria-labelledby={id} aria-modal="true" onCancel={e=>{e.preventDefault();}} onClick={e=>{if(e.target===e.currentTarget&&!busy){const r=e.currentTarget.getBoundingClientRect();if(e.clientX<r.left||e.clientX>r.right||e.clientY<r.top||e.clientY>r.bottom)onRequestClose();}}}><header className="modal-header"><h2 id={id}>{title}</h2><button className="control" disabled={busy} onClick={onRequestClose}>Close</button></header><div className="modal-content">{children}</div>{footer&&<footer className="modal-footer">{footer}</footer>}</dialog>,document.body);
}
