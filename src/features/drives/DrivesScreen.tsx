import {useCallback,useEffect,useRef,useState} from 'react';
import {errorMessage} from '../../app/backend';
import {useAppServices} from '../../app/AppServices';
import type {DriveDescriptor} from '../../app/contracts';
import {useOptionalSources} from '../library/SourceManagerProvider';
import {FolderManager} from '../library/FolderManager';
import {drivesApi,type EjectPreview} from './api';
import {DriveRow} from './DriveRow';
import {EjectDialog} from './EjectDialog';
import {PageHeader} from '../../ui/PageHeader';
import {Icon} from '../../ui/Icons';
import './drives.css';
export function DrivesScreen({api:override}:{api?:typeof drivesApi}){
 const services=useAppServices(),api=override??services.drives,sources=useOptionalSources();const refreshSources=sources?.refresh;
 const [drives,setDrives]=useState<DriveDescriptor[]>([]),[expanded,setExpanded]=useState<Record<string,boolean>>({}),[error,setError]=useState(''),[busy,setBusy]=useState(''),[message,setMessage]=useState(''),[folders,setFolders]=useState(false),[confirmation,setConfirmation]=useState<{driveId:string;preview:EjectPreview}|null>(null);const loaded=useRef(false),lock=useRef(false),generation=useRef(0);
 const observe=useCallback(async()=>{const ticket=++generation.current;const value=await api.list();if(ticket!==generation.current)return;const firstLoad=!loaded.current;setDrives(value);setExpanded(previous=>Object.fromEntries(value.map(d=>[d.id,previous[d.id]??(firstLoad&&d.id===value.find(x=>x.removable)?.id)])));loaded.current=true;},[api]);
 useEffect(()=>{if(!services.canUseMedia&&!override)return;void observe().catch(e=>setError(errorMessage(e)));return()=>{generation.current++;};},[observe,services.canUseMedia,override]);
 async function run(label:string,work:()=>Promise<void>){if(lock.current)return;lock.current=true;setBusy(label);setError('');setMessage('');try{await work();}catch(e){setError(errorMessage(e));try{await observe();await refreshSources?.();}catch{/* Preserve the action error. */}}finally{lock.current=false;setBusy('');}}
 async function eject(){if(!confirmation)return;const confirmed=confirmation;await run('Stopping media and safely removing the device',async()=>{const result=await api.eject(confirmed.driveId,confirmed.preview.driveIds);await observe();await refreshSources?.();setMessage(result.status==='powered_off'?'Safe to remove. The device has been powered off.':'All volumes are unmounted. Safe to disconnect the device; power-off is not supported.');});setConfirmation(null);}
 return <><PageHeader title="Drives & USB" actions={<button className="text-button" disabled={!!busy} onClick={()=>void run('Checking drives',observe)}><Icon name="refresh"/>Refresh</button>}/>{busy&&<p role="status">{busy}</p>}{message&&<p role="status">{message}</p>}{error&&<p role="alert" className="message">{error}</p>}{!busy&&!error&&!drives.length&&<p>No storage devices found.</p>}<div className="drive-list">{drives.map(d=><DriveRow key={d.id} drive={d} pending={!!busy} expanded={expanded[d.id]??false} onExpandedChange={open=>setExpanded(previous=>previous[d.id]===open?previous:{...previous,[d.id]:open})} onMount={id=>void run('Mounting volume',async()=>{await api.mount(id);await observe();await refreshSources?.();setMessage('Volume mounted.');})} onPrepareEject={id=>void run('Checking the whole device',async()=>setConfirmation({driveId:id,preview:await api.prepare(id)}))}/>)}</div><button className="disclosure-button manage-drives" onClick={()=>setFolders(true)}><span><Icon name="folder"/>Manage media folders</span><span aria-hidden="true">›</span></button><FolderManager open={folders} onRequestClose={()=>setFolders(false)}/><EjectDialog preview={confirmation?.preview??null} pending={!!busy} onRequestClose={()=>setConfirmation(null)} onConfirm={()=>void eject()}/></>;
}
