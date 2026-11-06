import {useEffect,useState} from 'react';
import type {MediaItem,PhotoImage} from '../../app/contracts';
import {useAppServices} from '../../app/AppServices';
import {errorMessage} from '../../app/backend';
import {rasterUrl} from './api';
import {Dialog} from '../../ui/Dialog';
export function PhotoViewer({item,onRequestClose}:{item:MediaItem;onRequestClose:()=>void}){
 const {photos}=useAppServices(),[image,setImage]=useState<PhotoImage|null>(null),[actual,setActual]=useState(false),[error,setError]=useState('');
 useEffect(()=>{let active=true;setImage(null);setActual(false);setError('');if(item.availability!=='available'){setError('Photo unavailable');return;}void photos.image(item.id).then(value=>{if(active)setImage(value);}).catch(e=>{if(active)setError(errorMessage(e));});return()=>{active=false;};},[photos,item.id,item.availability]);
 const url=rasterUrl(image?.image);
 return <Dialog open title={item.displayName} variant="viewer" onRequestClose={onRequestClose}><div className="actions"><button className="control" aria-pressed={!actual} onClick={()=>setActual(false)}>Fit</button><button className="control" aria-pressed={actual} onClick={()=>setActual(true)}>Actual size</button></div><div className={`photo-preview ${actual?'photo-actual':''}`}>{url?<img src={url} alt={item.displayName} style={actual?{width:image!.width,height:image!.height}:undefined}/>:<p role="status">{error||image?'Photo unavailable':'Loading photo'}</p>}</div>{error&&<p role="alert">{error}</p>}<details><summary>Photo details</summary><p>{item.displayName}</p>{image&&<p>Decoded dimensions: {image.width} × {image.height}</p>}{image?.modifiedAtMs!=null&&<p>Modified {new Date(image.modifiedAtMs).toLocaleDateString()}</p>}<p>Images are decoded at up to 1920 × 1080 pixels.</p></details></Dialog>;
}
