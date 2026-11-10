import {useEffect,useState} from 'react';
import type {PhotoFrame} from '../../app/contracts';
import {photosApi,rasterUrl} from './api';
import './photos.css';
/** This screen calls only the presentation capability. It never imports the main backend. */
export function PresentationScreen({api=photosApi}:{api?:typeof photosApi}){
 const [frame,setFrame]=useState<PhotoFrame|null>(null);
 useEffect(()=>{let active=true;let timer:ReturnType<typeof setTimeout>;async function refresh(){try{const value=await api.frame();if(active)setFrame(value);}catch{if(active)setFrame(null);}finally{if(active)timer=setTimeout(()=>void refresh(),500);}}void refresh();return()=>{active=false;clearTimeout(timer);};},[api]);
 const source=rasterUrl(frame?.image?.image);
 return <main className={`photo-presentation ${frame?.snapshot.fit==='actual'?'photo-actual':''}`}>{source?<img src={source} alt={`Photo ${(frame?.snapshot.index??0)+1} of ${frame?.snapshot.total??1}`}/>:<p role="status">Waiting for a photo</p>}{frame?.snapshot.error&&<p className="photo-presentation-status" role="alert">{frame.snapshot.error}</p>}</main>;
}
export default PresentationScreen;
