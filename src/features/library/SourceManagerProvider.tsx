import {createContext,useCallback,useContext,useEffect,useRef,useState,type ReactNode} from 'react';
import {useAppServices} from '../../app/AppServices';
import {errorMessage} from '../../app/backend';
import type {MediaSource} from '../../app/contracts';
export interface SourceManagerState {sources:MediaSource[];pendingScans:ReadonlySet<string>;errorBySource:ReadonlyMap<string,string>;refresh:()=>Promise<void>;scan:(id:string)=>Promise<void>;cancelScan:(id:string)=>Promise<void>;remove:(id:string)=>Promise<void>;revision:number}
const Context=createContext<SourceManagerState|null>(null);
export function useOptionalSources(){return useContext(Context);}
export function useSources(){const value=useContext(Context);if(!value)throw Error('SourceManagerProvider is required');return value;}
export function SourceManagerProvider({children}:{children:ReactNode}){
 const {backend:api,canUseMedia,observeSources}=useAppServices();const [sources,setSources]=useState<MediaSource[]>([]),[pendingScans,setPending]=useState<ReadonlySet<string>>(new Set()),[errorBySource,setErrors]=useState<ReadonlyMap<string,string>>(new Map()),[revision,setRevision]=useState(0);
 const active=useRef(true),generation=useRef(0),pending=useRef(new Set<string>()),rescan=useRef(new Set<string>()),currentSources=useRef<MediaSource[]|null>(null);
 const accept=useCallback((value:MediaSource[],force=false)=>{const changed=JSON.stringify(value)!==JSON.stringify(currentSources.current);if(changed||force){currentSources.current=value;setSources(value);setRevision(v=>v+1);}},[]);
 const fail=useCallback((id:string,reason:unknown)=>{if(active.current)setErrors(current=>new Map(current).set(id,errorMessage(reason)));},[]);
 const refresh=useCallback(async()=>{const ticket=++generation.current;const value=await api.sources();if(active.current&&ticket===generation.current){accept(value,true);}},[api,accept]);
 useEffect(()=>{active.current=true;if(!canUseMedia)return;let alive=true;let dispose:(()=>void)|undefined;
  void observeSources(value=>{if(alive){generation.current++;accept(value);}}).then(async stop=>{if(!alive){stop();return;}dispose=stop;await refresh();}).catch(e=>{if(alive)fail('',e);});
  return()=>{alive=false;active.current=false;generation.current++;dispose?.();};
 },[refresh,fail,canUseMedia,observeSources,accept]);
 const clear=(id:string)=>setErrors(current=>{const next=new Map(current);next.delete(id);return next;});
 async function scan(id:string){if(pending.current.has(id)){rescan.current.add(id);return;}pending.current.add(id);setPending(new Set(pending.current));clear(id);try{do{rescan.current.delete(id);await api.scanSource(id);await refresh();}while(active.current&&rescan.current.has(id));}catch(e){fail(id,e);}finally{rescan.current.delete(id);pending.current.delete(id);if(active.current)setPending(new Set(pending.current));}}
 async function cancelScan(id:string){rescan.current.delete(id);try{await api.cancelScan(id);}catch(e){fail(id,e);}}
 async function remove(id:string){clear(id);try{await api.removeSource(id);await refresh();}catch(e){fail(id,e);throw e;}}
 return <Context.Provider value={{sources,pendingScans,errorBySource,revision,refresh,scan,cancelScan,remove}}>{children}</Context.Provider>;
}
