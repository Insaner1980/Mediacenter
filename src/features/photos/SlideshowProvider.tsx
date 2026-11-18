import {createContext,useContext,useEffect,useRef,useState,type ReactNode} from 'react';
import {useAppServices} from '../../app/AppServices';
import {errorMessage} from '../../app/backend';
import type {SlideshowAction,SlideshowRequest,SlideshowSnapshot} from '../../app/contracts';
export interface SlideshowState {snapshot:SlideshowSnapshot|null;pending:boolean;error:string|null;start:(request:SlideshowRequest)=>Promise<void>;control:(action:SlideshowAction)=>Promise<void>}
const Context=createContext<SlideshowState|null>(null);
export function useOptionalSlideshow(){return useContext(Context);}
export function useSlideshow(){const value=useContext(Context);if(!value)throw Error('SlideshowProvider is required');return value;}
export function SlideshowProvider({children}:{children:ReactNode}){
 const {photos:api,canUseMedia}=useAppServices(),[snapshot,setSnapshot]=useState<SlideshowSnapshot|null>(null),[pending,setPending]=useState(false),[error,setError]=useState<string|null>(null);const active=useRef(true),epoch=useRef(0),chain=useRef<Promise<unknown>>(Promise.resolve()),busy=useRef(false);
 const [observationError,setObservationError]=useState<string|null>(null);
 useEffect(()=>{active.current=true;if(!canUseMedia)return;let alive=true;let timer:ReturnType<typeof setTimeout>;
  async function poll(){const ticket=epoch.current;const work=chain.current.then(()=>api.snapshot());chain.current=work.catch(()=>{});try{const value=await work;if(alive&&ticket===epoch.current){setSnapshot(value??null);setObservationError(null);}}catch(e){if(alive&&ticket===epoch.current)setObservationError(errorMessage(e));}finally{if(alive)timer=setTimeout(()=>void poll(),1000);}}
  void poll();return()=>{alive=false;active.current=false;epoch.current++;clearTimeout(timer);};
 },[api,canUseMedia]);
 async function run(work:()=>Promise<SlideshowSnapshot>){if(busy.current)return;busy.current=true;epoch.current++;setPending(true);setError(null);const next=chain.current.then(work);chain.current=next.catch(()=>{});try{const value=await next;if(active.current){setSnapshot(value);setObservationError(null);}}catch(e){if(active.current)setError(errorMessage(e));throw e;}finally{busy.current=false;if(active.current)setPending(false);}}
 return <Context.Provider value={{snapshot,pending,error:error??observationError,start:request=>run(()=>api.start(request)),control:action=>run(()=>api.control(action))}}>{children}</Context.Provider>;
}
