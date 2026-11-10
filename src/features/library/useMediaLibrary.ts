import {RemoteNavigationOrigin} from '../../app/navigation';
import {useCallback,useEffect,useRef,useState,useContext} from 'react';
import {useAppServices} from '../../app/AppServices';
import {backend,errorMessage} from '../../app/backend';
import type {MediaKind,MediaPage,MediaSource} from '../../app/contracts';
import {useOptionalSources} from './SourceManagerProvider';

export function useMediaLibrary(kind:MediaKind,apiOverride?:typeof backend){
 const remoteNavigation=useContext(RemoteNavigationOrigin);
 const services=useAppServices(),api=apiOverride??services.backend,shared=useOptionalSources();
 const [query,setQueryValue]=useState(''),[sourceId,setSource]=useState<string|null>(null),[cursor,setCursor]=useState<string|null>(null),[history,setHistory]=useState<(string|null)[]>([]),[page,setPage]=useState<MediaPage>({items:[],nextCursor:null}),[sources,setSources]=useState<MediaSource[]>([]),[loading,setLoading]=useState(true),[stale,setStale]=useState(true),[error,setError]=useState<string|null>(null),[actionError,setActionError]=useState<string|null>(null),[version,setVersion]=useState(0),[initializing,setInitializing]=useState(true),[opening,setOpening]=useState(false);
 const sharedRef=useRef(shared),selection=useRef(0),openLock=useRef(false),mounted=useRef(true);
 useEffect(()=>{sharedRef.current=shared;},[shared]);
 useEffect(()=>{mounted.current=true;return()=>{mounted.current=false;};},[]);
 const revision=shared?.revision??0;
 const reset=useCallback(()=>{setCursor(null);setHistory([]);setLoading(true);setStale(true);},[]);
 const scan=useCallback(async(id:string)=>{
  const owner=sharedRef.current;
  if(owner){await owner.scan(id);return;}
  try{await api.scanSource(id);}catch(e){if(mounted.current)setActionError(errorMessage(e));}
  finally{if(mounted.current)setVersion(v=>v+1);}
 },[api]);
 useEffect(()=>{
  if(!services.canUseMedia&&!apiOverride){setInitializing(false);setLoading(false);return;}
  if(remoteNavigation){setInitializing(false);return;}
  let active=true;const ticket=selection.current;setInitializing(true);
  void (api.ensureDefaultSource?.(kind)??Promise.resolve(null)).then(async id=>{
   if(!active||ticket!==selection.current)return;
   if(id){setSource(id);reset();setInitializing(false);await sharedRef.current?.refresh();if(active)void scan(id);}
  }).catch(e=>{if(active&&ticket===selection.current)setActionError(errorMessage(e));}).finally(()=>{if(active)setInitializing(false);});
  return()=>{active=false;};
 },[api,apiOverride,services.canUseMedia,kind,reset,scan,remoteNavigation]);
 useEffect(()=>{let active=true;if(shared){setSources(shared.sources);return;}if(!services.canUseMedia&&!apiOverride){setLoading(false);return;}void api.sources().then(value=>{if(active)setSources(value);}).catch(e=>{if(active)setError(errorMessage(e));});return()=>{active=false;};},[api,apiOverride,services.canUseMedia,shared,version]);
 useEffect(()=>{reset();},[revision,reset]);
 useEffect(()=>{
  if(initializing)return;
  if(!services.canUseMedia&&!apiOverride){setLoading(false);return;}
  let active=true;setLoading(true);setStale(true);setError(null);
  const timer=setTimeout(()=>{void api.queryMedia(kind,sourceId,query,cursor).then(result=>{if(active){setPage(result);setStale(false);}}).catch(e=>{if(active)setError(errorMessage(e));}).finally(()=>{if(active)setLoading(false);});},200);
  return()=>{active=false;clearTimeout(timer);};
 },[api,apiOverride,services.canUseMedia,kind,sourceId,query,cursor,version,revision,initializing]);
 function select(value:string|null){selection.current++;setSource(value);setInitializing(false);setActionError(null);reset();}
 async function openFolder(){
  if(openLock.current)return;openLock.current=true;setOpening(true);setActionError(null);
  try{
   const grant=await api.chooseSource();if(!grant||!mounted.current)return;
   const id=await api.addSource(grant.grantId,[kind]);
   if(mounted.current){select(id);setQueryValue('');}
   await sharedRef.current?.refresh();void scan(id);
  }catch(e){if(mounted.current)setActionError(errorMessage(e));}
  finally{openLock.current=false;if(mounted.current)setOpening(false);}
 }
 return {items:page.items,sources,query,sourceId,loading:loading||initializing,stale:stale||initializing,opening,error:actionError||error||(sourceId?shared?.errorBySource.get(sourceId):null)||null,scanning:!!sourceId&&!!shared?.pendingScans.has(sourceId),hasPrevious:history.length>0,hasNext:!!page.nextCursor,openFolder,cancelScan(){if(sourceId)void shared?.cancelScan(sourceId);},setQuery(value:string){setQueryValue(Array.from(value).slice(0,200).join(''));reset();},setSourceId:select,previousPage(){setCursor(history.at(-1)??null);setHistory(v=>v.slice(0,-1));setLoading(true);},nextPage(){if(page.nextCursor){setHistory(v=>[...v,cursor]);setCursor(page.nextCursor);setLoading(true);}},refresh(){setActionError(null);reset();setVersion(v=>v+1);if(sourceId)void scan(sourceId);else if(shared)void shared.refresh().catch(e=>setError(errorMessage(e)));}};
}
export type MediaLibraryState=ReturnType<typeof useMediaLibrary>;
