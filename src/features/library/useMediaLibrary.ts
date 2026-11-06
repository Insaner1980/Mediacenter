import {useCallback,useEffect,useState} from 'react';
import {useAppServices} from '../../app/AppServices';
import {backend,errorMessage} from '../../app/backend';
import type {MediaKind,MediaPage,MediaSource} from '../../app/contracts';
import {useOptionalSources} from './SourceManagerProvider';
export function useMediaLibrary(kind:MediaKind,apiOverride?:typeof backend){
 const services=useAppServices(),api=apiOverride??services.backend,shared=useOptionalSources();
 const [query,setQueryValue]=useState(''),[sourceId,setSource]=useState<string|null>(null),[cursor,setCursor]=useState<string|null>(null),[history,setHistory]=useState<(string|null)[]>([]),[page,setPage]=useState<MediaPage>({items:[],nextCursor:null}),[sources,setSources]=useState<MediaSource[]>([]),[loading,setLoading]=useState(true),[error,setError]=useState<string|null>(null),[version,setVersion]=useState(0);
 const revision=shared?.revision??0;
 const reset=useCallback(()=>{setCursor(null);setHistory([]);setLoading(true);},[]);
 useEffect(()=>{let active=true;if(shared){setSources(shared.sources);return;}if(!services.canUseMedia&&!apiOverride){setLoading(false);return;}void api.sources().then(value=>{if(active)setSources(value);}).catch(e=>{if(active)setError(errorMessage(e));});return()=>{active=false;};},[api,apiOverride,services.canUseMedia,shared,version]);
 useEffect(()=>{reset();},[revision,reset]);
 useEffect(()=>{
  if(!services.canUseMedia&&!apiOverride){setLoading(false);return;}let active=true;setLoading(true);setError(null);
  const timer=setTimeout(()=>{void api.queryMedia(kind,sourceId,query,cursor).then(result=>{if(active)setPage(result);}).catch(e=>{if(active)setError(errorMessage(e));}).finally(()=>{if(active)setLoading(false);});},200);
  return()=>{active=false;clearTimeout(timer);};
 },[api,apiOverride,services.canUseMedia,kind,sourceId,query,cursor,version,revision]);
 return {items:page.items,sources,query,sourceId,loading,error,hasPrevious:history.length>0,hasNext:!!page.nextCursor,setQuery(value:string){setQueryValue(Array.from(value).slice(0,200).join(''));reset();},setSourceId(value:string|null){setSource(value);reset();},previousPage(){setCursor(history.at(-1)??null);setHistory(v=>v.slice(0,-1));setLoading(true);},nextPage(){if(page.nextCursor){setHistory(v=>[...v,cursor]);setCursor(page.nextCursor);setLoading(true);}},refresh(){reset();setVersion(v=>v+1);if(shared)void shared.refresh().catch(e=>setError(errorMessage(e)));}};
}
export type MediaLibraryState=ReturnType<typeof useMediaLibrary>;
