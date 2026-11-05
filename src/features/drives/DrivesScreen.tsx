import {useEffect,useState} from 'react';
import {backend,errorMessage} from '../../app/backend';
import type {DriveDescriptor} from '../../app/contracts';
import {SourcePicker} from '../library/SourcePicker';
export function DrivesScreen(){
 const [drives,setDrives]=useState<DriveDescriptor[]>([]);const [error,setError]=useState('');const [busy,setBusy]=useState(false);
 async function refresh(){setBusy(true);setError('');try{setDrives(await backend.drives());}catch(reason){setError(errorMessage(reason));}finally{setBusy(false);}}
 useEffect(()=>{void refresh();},[]);
 return <><button className="control" disabled={busy} onClick={()=>void refresh()}>Refresh drives</button>{busy && <p role="status">Checking drives</p>}{error && <p role="alert" className="message">{error}</p>}<div className="media-list">{drives.map(d=><article key={d.id}><h2>{d.name||'Storage device'}</h2><p>{d.removable?'Removable storage':'Internal storage'}</p>{d.volumes.map(v=><section key={v.id}><strong>{v.label||'Volume'}</strong><p>{(v.size/1024**3).toFixed(1)} GiB total</p><p>{v.ambiguous?'Ambiguous volume identity':v.mounted?'Mounted':'Unmounted'}</p>{v.mountPoints.map(path=><p key={path}>{path}</p>)}</section>)}</article>)}</div><SourcePicker/></>;
}
