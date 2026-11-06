import {useEffect,useRef,useState} from 'react';
import type {MediaItem} from '../../app/contracts';
import {useAppServices} from '../../app/AppServices';
import {Icon} from '../../ui/Icons';
import {createThumbnailLoader} from './mediaImages';
import {useOptionalSources} from './SourceManagerProvider';
const loaders=new WeakMap<object,ReturnType<typeof createThumbnailLoader>>();
export function MediaThumbnail({item,alt=''}:{item:MediaItem;alt?:string}){
 const {backend}=useAppServices(),sources=useOptionalSources(),ref=useRef<HTMLDivElement>(null);const [url,setUrl]=useState<string|null>(null);const revision=sources?.revision??0;
 useEffect(()=>{let active=true;setUrl(null);if(item.availability!=='available')return;
  let load=loaders.get(backend);if(!load){load=createThumbnailLoader(backend.thumbnail);loaders.set(backend,load);}const run=()=>{void load!(`${item.id}:${item.thumbnailId}:${item.modifiedAtMs}:${revision}`,item.id).then(value=>{if(active)setUrl(value);});};
  if(typeof IntersectionObserver==='undefined'){run();return()=>{active=false;};}
  const observer=new IntersectionObserver(entries=>{if(entries.some(e=>e.isIntersecting)){observer.disconnect();run();}},{rootMargin:'200px'});if(ref.current)observer.observe(ref.current);return()=>{active=false;observer.disconnect();};
 },[backend,item.id,item.thumbnailId,item.modifiedAtMs,item.availability,revision]);
 return <div ref={ref} className="media-thumbnail">{url?<img src={url} alt={alt}/>:<Icon name={item.kind==='photo'?'photos':'video'}/>}</div>;
}
