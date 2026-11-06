import {createContext,useCallback,useContext,useEffect,useRef,useState,type ReactNode} from 'react';
import {useAppServices} from '../../app/AppServices';
import {errorMessage} from '../../app/backend';
import type {MusicAction,MusicQueue,PlayerAction,PlayerSnapshot} from '../../app/contracts';
export interface PlaybackState {player:PlayerSnapshot|null;queue:MusicQueue|null;error:string|null;pending:boolean;playerAction:(action:PlayerAction)=>Promise<void>;musicAction:(action:MusicAction)=>Promise<void>;refresh:()=>Promise<void>}
const Context=createContext<PlaybackState|null>(null);
export function usePlayback(){const value=useContext(Context);if(!value)throw Error('PlaybackProvider is required');return value;}
export function useOptionalPlayback(){return useContext(Context);}
export function PlaybackProvider({children}:{children:ReactNode}){
 const {backend,music,player:observation,canUseMedia}=useAppServices();
 const [player,setPlayer]=useState<PlayerSnapshot|null>(null),[queue,setQueue]=useState<MusicQueue|null>(null),[error,setError]=useState<string|null>(null),[pending,setPending]=useState(false);
 const live=useRef(true),playerRef=useRef(player),queueRef=useRef(queue),playerEpoch=useRef(0),queueEpoch=useRef(0),busy=useRef(false);
 const acceptPlayer=useCallback((value:PlayerSnapshot|null)=>{if(!live.current)return;const previous=playerRef.current;if(value&&previous&&value.sessionId===previous.sessionId&&value.revision<previous.revision)return;playerRef.current=value;setPlayer(value);},[]);
 const acceptQueue=useCallback((value:MusicQueue)=>{if(!live.current||!value)return;const previous=queueRef.current;if(previous&&value.revision<previous.revision)return;queueRef.current=value;setQueue(value);},[]);
 const refresh=useCallback(async()=>{const p=playerEpoch.current,q=queueEpoch.current;await Promise.all([backend.player().then(value=>{if(p===playerEpoch.current)acceptPlayer(value??null);}),music.queue().then(value=>{if(q===queueEpoch.current)acceptQueue(value);})]);},[backend,music,acceptPlayer,acceptQueue]);
 useEffect(()=>{live.current=true;if(!canUseMedia)return;let active=true;const dispose:(()=>void)[]=[];
 const fail=(reason:unknown)=>{if(active)setError(errorMessage(reason));};
 void observation.observe(value=>{if(active){playerEpoch.current++;acceptPlayer(value);}}).then(stop=>{if(active)dispose.push(stop);else stop();}).catch(fail);
 void music.subscribe(value=>{if(active){queueEpoch.current++;acceptQueue(value);}}).then(async stop=>{if(!active){stop();return;}dispose.push(stop);const epoch=queueEpoch.current;const value=await music.queue();if(active&&epoch===queueEpoch.current)acceptQueue(value);}).catch(fail);
 return()=>{active=false;live.current=false;dispose.forEach(stop=>stop());};
 },[music,observation,canUseMedia,acceptPlayer,acceptQueue]);
 // Volume updates share one flight; the last requested value is always flushed.
 const volume=useRef<{sessionId:string;value:number}|null>(null),volumeFlight=useRef<Promise<void>|null>(null);
 async function playerAction(action:PlayerAction){
  const current=playerRef.current;if(!current)return;
  if(action.type==='volume'){
   volume.current={sessionId:current.sessionId,value:action.value};
   if(!volumeFlight.current){volumeFlight.current=(async()=>{try{do{while(volume.current){const next=volume.current;volume.current=null;await backend.controlPlayer(next.sessionId,{type:'volume',value:next.value});}await refresh();}while(volume.current);}catch(reason){if(live.current)setError(errorMessage(reason));}finally{volumeFlight.current=null;}})();}
   return volumeFlight.current;
  }
  if(busy.current)return;busy.current=true;setPending(true);setError(null);
  try{await backend.controlPlayer(current.sessionId,action);await refresh();}catch(reason){if(live.current)setError(errorMessage(reason));}finally{busy.current=false;if(live.current)setPending(false);}
 }
 async function musicAction(action:MusicAction){const current=queueRef.current;if(!current||busy.current)return;busy.current=true;setPending(true);setError(null);const epoch=queueEpoch.current;
  try{const value=await music.control(current,action);if(epoch===queueEpoch.current)acceptQueue(value);}catch(reason){if(live.current)setError(errorMessage(reason));const recovery=queueEpoch.current;try{const value=await music.queue();if(recovery===queueEpoch.current)acceptQueue(value);}catch{/* Retain the mutation failure. */}}finally{busy.current=false;if(live.current)setPending(false);}
 }
 return <Context.Provider value={{player,queue,error,pending,playerAction,musicAction,refresh}}>{children}</Context.Provider>;
}
