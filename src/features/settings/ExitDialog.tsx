import {useEffect,useState} from 'react';
import {isTauri,invoke} from '@tauri-apps/api/core';
import {listen} from '@tauri-apps/api/event';
import type {ExitStatus,ExitAction,AppError} from '../../app/contracts';
import {errorMessage} from '../../app/backend';
import {Dialog} from '../../ui/Dialog';
export function ExitDialog(){
 const [status,setStatus]=useState<ExitStatus|null>(null);const [error,setError]=useState('');const [busy,setBusy]=useState(false);
 useEffect(()=>{if(!isTauri())return;let live=true;const dispose:(()=>void)[]=[];void Promise.all([listen<ExitStatus>('exit_requested',({payload})=>{if(live){setStatus(payload);setError('');}}),listen<AppError>('exit_error',({payload})=>{if(live)setError(payload.message);})]).then(stops=>{if(live)dispose.push(...stops);else stops.forEach(stop=>stop());});return()=>{live=false;dispose.forEach(stop=>stop());};},[]);
 async function action(action:ExitAction){if(busy)return;setBusy(true);setError('');try{await invoke('request_exit',{action});if(action==='keep_open')setStatus(null);}catch(reason){setError(errorMessage(reason));}finally{setBusy(false);}}
 if(!status)return error?<p role="alert" className="message">{error}</p>:null;
 const local=status.localActive||status.backgroundActive;
 return <Dialog open title="Close Mediacenter?" busy={busy} onRequestClose={()=>void action('keep_open')}><p>{local?'Local playback or background work is active. Stopping closes this application’s players, slideshow and pending media work.':'A managed service window is open.'}</p><div className="actions"><button className="control" disabled={busy} onClick={()=>void action('keep_open')}>{local?'Keep open':'Cancel'}</button>{local?<button className="control" disabled={busy} onClick={()=>void action('stop_playback_and_quit')}>Stop playback and quit</button>:<><button className="control" disabled={busy} onClick={()=>void action('leave_service_open_and_quit')}>Leave service open and quit</button><button className="control" disabled={busy} onClick={()=>void action('close_service_and_quit')}>Close service and quit</button></>}</div>{error && <p role="alert" className="message">{error}</p>}</Dialog>;
}
