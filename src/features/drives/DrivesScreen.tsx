import {useCallback,useEffect,useState} from 'react';
import {errorMessage} from '../../app/backend';
import type {DriveDescriptor} from '../../app/contracts';
import {SourcePicker} from '../library/SourcePicker';
import {drivesApi,type EjectPreview} from './api';
export function DrivesScreen({api=drivesApi}:{api?:typeof drivesApi}){
 const [sourceRevision,setSourceRevision]=useState(0);
 const [drives,setDrives]=useState<DriveDescriptor[]>([]);
 const [error,setError]=useState('');
 const [busy,setBusy]=useState('');
 const [message,setMessage]=useState('');
 const [confirmation,setConfirmation]=useState<{driveId:string;preview:EjectPreview}|null>(null);
 const refresh=useCallback(async()=>{setBusy('Checking drives');setError('');setMessage('');setConfirmation(null);try{setDrives(await api.list());setSourceRevision(value=>value+1);}catch(reason){setError(errorMessage(reason));}finally{setBusy('');}},[api]);
 useEffect(()=>{void refresh();},[refresh]);
 async function mount(volumeId:string){
  setBusy('Mounting volume');setError('');setMessage('');setConfirmation(null);
  try{await api.mount(volumeId);setDrives(await api.list());setSourceRevision(value=>value+1);setMessage('Volume mounted.');}catch(reason){setError(errorMessage(reason));}finally{setBusy('');}
 }
 async function prepare(driveId:string){
  setBusy('Checking the whole device');setError('');setMessage('');setConfirmation(null);
  try{setConfirmation({driveId,preview:await api.prepare(driveId)});}catch(reason){setError(errorMessage(reason));}finally{setBusy('');}
 }
 async function eject(){
  if(!confirmation)return;
  setBusy('Stopping media and safely removing the device');setError('');setMessage('');
  try{
   const result=await api.eject(confirmation.driveId,confirmation.preview.driveIds);
   setDrives(await api.list());setSourceRevision(value=>value+1);
   setMessage(result.status==='powered_off'?'Safe to remove. The device has been powered off.':'All volumes are unmounted. Safe to disconnect the device; power-off is not supported.');
  }catch(reason){setError(errorMessage(reason));try{setDrives(await api.list());setSourceRevision(value=>value+1);}catch{/* Keep the original removal error visible. */}}
  finally{setBusy('');setConfirmation(null);}
 }
 return <>
  <button className="control" disabled={!!busy} onClick={()=>void refresh()}>Refresh drives</button>
  {busy && <p role="status">{busy}</p>}{message && <p role="status">{message}</p>}
  {error && <p role="alert" className="message">{error}</p>}
  {confirmation && <section aria-label="Confirm safe removal"><h2>Remove the whole device</h2>
   <p>{confirmation.preview.driveNames.join(', ')}</p>
   {confirmation.preview.driveIds.length>1 && <p>These drives share a physical device. Removing it affects every listed drive.</p>}
   <p>{confirmation.preview.volumeIds.length} volumes will be checked. Mediacenter will stop playback, slideshows, scans and image loading on this device, then wait for its files to close.</p>
   {confirmation.preview.activeUses.length>0 && <div aria-label="Active use"><h3>Currently using this device</h3>{confirmation.preview.activeUses.map(use=><p key={use}>{use}</p>)}</div>}
   <p>Another application may still prevent removal. Library records remain available offline.</p>
   <div className="actions"><button className="control" disabled={!!busy} onClick={()=>void eject()}>Stop using and safely remove</button><button className="control" disabled={!!busy} onClick={()=>setConfirmation(null)}>Cancel</button></div>
  </section>}
  {!busy && !error && drives.length===0 && <p>No storage devices found.</p>}
  <div className="media-list">{drives.map(d=><article key={d.id}><h2>{d.name||'Storage device'}</h2><p>{d.removable?'Removable storage':'Internal storage'}</p>
   {d.volumes.map(v=><section key={v.id}><strong>{v.label||'Volume'}</strong><p>{(v.size/1024**3).toFixed(1)} GiB total</p><p>{v.ambiguous?'Ambiguous volume identity':v.mounted?'Mounted':'Unmounted'}</p>{v.mountPoints.map(path=><p key={path}>{path}</p>)}{!v.mounted && d.removable && <button className="control" aria-label={`Mount ${v.label||'volume'}`} disabled={!!busy} onClick={()=>void mount(v.id)}>Mount</button>}</section>)}
   {d.removable && <button className="control" disabled={!!busy} onClick={()=>void prepare(d.id)}>Safely remove</button>}
  </article>)}</div><SourcePicker key={sourceRevision}/>
 </>;
}
