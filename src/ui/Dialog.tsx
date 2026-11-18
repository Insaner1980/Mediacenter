import {useEffect,useId,useRef,type ReactNode,type RefObject} from 'react';
import {createPortal} from 'react-dom';
import './controls.css';
export interface DialogProps {open:boolean;title:string;onRequestClose:()=>void;children:ReactNode;footer?:ReactNode;initialFocusRef?:RefObject<HTMLElement|null>;busy?:boolean;variant?:'sheet'|'dialog'|'viewer'|'popover';remoteConfirmation?:string;remoteDismiss?:boolean}
const focusable='button:not(:disabled), input:not(:disabled), select:not(:disabled), textarea:not(:disabled), summary, a[href], [tabindex="0"]';
const backgroundInert=new Map<HTMLElement,boolean>();
const modalStack:HTMLDialogElement[]=[];
const remoteClosers=new Map<HTMLDialogElement,()=>boolean>();
export function topmostDialog(){return modalStack.at(-1);}
export function closeRemoteDialog(){const top=topmostDialog();if(!top)return 'none';const close=remoteClosers.get(top);if(!close||!close())return 'blocked';return 'closed';}
let rootOpener:HTMLElement|null=null;
function syncModalBackground(){
 const top=topmostDialog();
 for(const [element,original] of backgroundInert){
  if(!top||element===top||!element.isConnected){element.inert=original;backgroundInert.delete(element);}
 }
 if(!top)return;
 for(const element of document.body.children){
  if(!(element instanceof HTMLElement)||element===top||backgroundInert.has(element))continue;
  backgroundInert.set(element,element.inert);
  element.inert=true;
 }
}
export function Dialog({open,title,onRequestClose,children,footer,initialFocusRef,busy=false,variant='dialog',remoteConfirmation,remoteDismiss=false}:DialogProps){
 const ref=useRef<HTMLDialogElement>(null),close=useRef(onRequestClose),blocked=useRef(busy);const id=useId();
 useEffect(()=>{close.current=onRequestClose;blocked.current=busy;},[onRequestClose,busy]);
 useEffect(()=>{
  if(!open)return;const element=ref.current!;const opener=document.activeElement as HTMLElement|null;
  if(!modalStack.length)rootOpener=opener;
  modalStack.push(element);
  syncModalBackground();
  if(typeof element.showModal==='function')element.showModal();else element.setAttribute('open','');
  if(remoteDismiss)remoteClosers.set(element,()=>{if(blocked.current)return false;close.current();return true;});
  (initialFocusRef?.current??element.querySelector<HTMLElement>(focusable)??element).focus();
  function key(event:KeyboardEvent){if(event.key==='Escape'){event.preventDefault();event.stopPropagation();if(!blocked.current)close.current();}if(event.key==='Tab'){const items=Array.from(element.querySelectorAll<HTMLElement>(focusable)).filter(e=>!e.closest('details:not([open])')||e.tagName==='SUMMARY');const first=items[0],last=items.at(-1);if(!first){event.preventDefault();element.focus();}else if(event.shiftKey&&(document.activeElement===first||document.activeElement===element)){event.preventDefault();last?.focus();}else if(!event.shiftKey&&document.activeElement===last){event.preventDefault();first.focus();}}}
  element.addEventListener('keydown',key);
  return()=>{
   const wasTop=topmostDialog()===element;
   remoteClosers.delete(element);
   element.removeEventListener('keydown',key);
   if(typeof element.close==='function')element.close();else element.removeAttribute('open');
   modalStack.splice(modalStack.indexOf(element),1);
   syncModalBackground();
   if(!modalStack.length){if(rootOpener?.isConnected)rootOpener.focus();rootOpener=null;}
   else if(wasTop){
    if(opener?.isConnected&&!opener.closest('[inert]'))opener.focus();
    else topmostDialog()?.focus();
   }
  };
 },[open,initialFocusRef,remoteDismiss]);
 if(!open)return null;
 return createPortal(<dialog data-remote-confirmation={remoteConfirmation} data-remote-modal={remoteConfirmation??`modal-${id.replace(/[^a-zA-Z0-9_-]/g,'')}`} ref={ref} tabIndex={-1} className={`modal modal-${variant}`} aria-labelledby={id} aria-modal="true" onCancel={e=>{e.preventDefault();}} onClick={e=>{if(e.target===e.currentTarget&&!busy){const r=e.currentTarget.getBoundingClientRect();if(e.clientX<r.left||e.clientX>r.right||e.clientY<r.top||e.clientY>r.bottom)onRequestClose();}}}><header className="modal-header"><h2 id={id}>{title}</h2><button data-remote-confirm={remoteConfirmation} data-remote-accept="false" data-remote-id={remoteDismiss||remoteConfirmation?`close-${id.replace(/[^a-zA-Z0-9_-]/g,'')}`:undefined} data-remote-local="dismiss" data-remote-group="modal" className="control" disabled={busy} onClick={onRequestClose}>Close</button></header><div className="modal-content">{children}</div>{footer&&<footer className="modal-footer">{footer}</footer>}</dialog>,document.body);
}
