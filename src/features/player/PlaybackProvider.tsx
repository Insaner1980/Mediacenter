import {createContext,useCallback,useContext,useEffect,useRef,useState,type ReactNode} from 'react';
import {useAppServices} from '../../app/AppServices';
import {errorMessage} from '../../app/backend';
import type {MusicAction,MusicQueue,PlayerAction,PlayerSnapshot} from '../../app/contracts';
export interface PlaybackState {player:PlayerSnapshot|null;queue:MusicQueue|null;error:string|null;queueError:string|null;pending:boolean;playerAction:(action:PlayerAction,sessionId?:string)=>Promise<void>;musicAction:(action:MusicAction)=>Promise<void>;refresh:()=>Promise<void>;refreshQueue:()=>Promise<void>}
const Context=createContext<PlaybackState|null>(null);
export function usePlayback(){const value=useContext(Context);if(!value)throw Error('PlaybackProvider is required');return value;}
export function useOptionalPlayback(){return useContext(Context);}
function subscribeWithRetry<T>(
 subscribe:(update:(value:T)=>void)=>Promise<()=>void>,
 accept:(value:T)=>void,
 onError:(reason:unknown)=>void,
 onReady:()=>void,
){
 let active=true,delay=250;
 let stop:(()=>void)|undefined,timer:ReturnType<typeof setTimeout>|undefined;
 async function connect(){
  let receiving=true;
  let dispose:()=>void;
  try{dispose=await subscribe(value=>{if(active&&receiving)accept(value);});}
  catch(reason){
   receiving=false;
   if(!active)return;
   onError(reason);
   timer=setTimeout(()=>void connect(),delay);
   delay=Math.min(delay*2,5000);
   return;
  }
  if(!active){dispose();return;}
  stop=dispose;
  onReady();
 }
 void connect();
 return()=>{active=false;clearTimeout(timer);stop?.();};
}
export function PlaybackProvider({children}:{children:ReactNode}){
 const {backend,music,player:observation,canUseMedia}=useAppServices();
 const [player,setPlayer]=useState<PlayerSnapshot|null>(null),[queue,setQueue]=useState<MusicQueue|null>(null),[error,setError]=useState<string|null>(null),[pending,setPending]=useState(false);
 const [observationError,setObservationError]=useState<string|null>(null),[queueError,setQueueError]=useState<string|null>(null),[queueSubscriptionError,setQueueSubscriptionError]=useState<string|null>(null);
 const live=useRef(true),playerRef=useRef(player),queueRef=useRef(queue),playerEpoch=useRef(0),queueEpoch=useRef(0),busy=useRef(false);
 const acceptPlayer=useCallback((value:PlayerSnapshot|null)=>{if(!live.current)return;const previous=playerRef.current;if(value&&previous&&value.sessionId===previous.sessionId&&value.revision<previous.revision)return;playerRef.current=value;setPlayer(value);},[]);
 const acceptQueue=useCallback((value:MusicQueue)=>{if(!live.current||!value)return;const previous=queueRef.current;if(previous&&value.revision<previous.revision)return;queueRef.current=value;setQueue(value);setQueueError(null);},[]);
 const refresh=useCallback(async()=>{const p=playerEpoch.current,q=queueEpoch.current;await Promise.all([backend.player().then(value=>{if(p===playerEpoch.current)acceptPlayer(value??null);}),music.queue().then(value=>{if(q===queueEpoch.current)acceptQueue(value);})]);},[backend,music,acceptPlayer,acceptQueue]);
 const refreshQueue=useCallback(async()=>{
  const epoch=++queueEpoch.current;
  try{const value=await music.queue();if(epoch===queueEpoch.current)acceptQueue(value);}
  catch(reason){if(live.current&&epoch===queueEpoch.current)setQueueError(errorMessage(reason));}
 },[music,acceptQueue]);
 useEffect(()=>{
  live.current=true;
  if(!canUseMedia)return;
  const stopPlayer=subscribeWithRetry(
   observation.observe,value=>{playerEpoch.current++;acceptPlayer(value);},
   reason=>setObservationError(errorMessage(reason)),()=>setObservationError(null),
  );
  const stopQueue=subscribeWithRetry(
   music.subscribe,value=>{queueEpoch.current++;acceptQueue(value);},
   reason=>setQueueSubscriptionError(errorMessage(reason)),
   ()=>{setQueueSubscriptionError(null);void refreshQueue();},
  );
  return()=>{live.current=false;queueEpoch.current++;stopPlayer();stopQueue();};
 },[music,observation,canUseMedia,acceptPlayer,acceptQueue,refreshQueue]);
 // Volume updates share one flight; the last requested value is always flushed.
 const volume=useRef<{sessionId:string;value:number}|null>(null),volumeFlight=useRef<Promise<void>|null>(null);
 async function playerAction(action:PlayerAction,sessionId?:string){
  const current=playerRef.current;if(!current||(sessionId!==undefined&&sessionId!==current.sessionId))return;
  if(action.type==='volume'){
   volume.current={sessionId:current.sessionId,value:action.value};
   if(!volumeFlight.current){
    volumeFlight.current=(async()=>{
     let failure:string|null=null;
     try{
      do{
       while(volume.current){
        const next=volume.current;volume.current=null;
        try{await backend.controlPlayer(next.sessionId,{type:'volume',value:next.value});failure=null;}
        catch(reason){failure=errorMessage(reason);}
       }
       try{await refresh();}catch(reason){failure??=errorMessage(reason);}
      }while(volume.current);
      if(failure&&live.current)setError(failure);
     }finally{volumeFlight.current=null;}
    })();
   }
   return volumeFlight.current;
  }
  if(busy.current)return;busy.current=true;setPending(true);setError(null);
  try{await backend.controlPlayer(current.sessionId,action);await refresh();}catch(reason){if(live.current)setError(errorMessage(reason));}finally{busy.current=false;if(live.current)setPending(false);}
 }
 async function musicAction(action:MusicAction){const current=queueRef.current;if(!current||busy.current)return;busy.current=true;setPending(true);setError(null);const epoch=queueEpoch.current;
  try{const value=await music.control(current,action);if(epoch===queueEpoch.current)acceptQueue(value);}catch(reason){if(live.current)setError(errorMessage(reason));const recovery=queueEpoch.current;try{const value=await music.queue();if(recovery===queueEpoch.current)acceptQueue(value);}catch{/* Retain the mutation failure. */}}finally{busy.current=false;if(live.current)setPending(false);}
 }
 return <Context.Provider value={{player,queue,error:error??observationError,queueError:queueError??queueSubscriptionError,pending,playerAction,musicAction,refresh,refreshQueue}}>{children}</Context.Provider>;
}
