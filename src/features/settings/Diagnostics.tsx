import {useState} from 'react';
import {invoke} from '@tauri-apps/api/core';
import type {DiagnosticReport} from '../../app/contracts';
import {errorMessage} from '../../app/backend';
export function Diagnostics(){
 const [report,setReport]=useState<DiagnosticReport|null>(null);const [busy,setBusy]=useState(false);const [message,setMessage]=useState('');
 async function preview(){setBusy(true);setMessage('');try{setReport(await invoke<DiagnosticReport>('get_diagnostics'));}catch(reason){setMessage(errorMessage(reason));}finally{setBusy(false);}}
 async function save(){setBusy(true);setMessage('');try{if(await invoke<boolean>('export_diagnostics'))setMessage('Diagnostics saved.');}catch(reason){setMessage(errorMessage(reason));}finally{setBusy(false);}}
 return <section><h2>Diagnostics</h2><p>Preview dependency versions and application state. Reports contain no browser profiles, credentials, public IP addresses or media paths.</p><div className="actions"><button className="control" disabled={busy} onClick={()=>void preview()}>Preview diagnostics</button>{report && <button className="control" disabled={busy} onClick={()=>void save()}>Save diagnostics report</button>}</div>{report && <pre className="diagnostic-preview">{JSON.stringify(report,null,2)}</pre>}{message && <p role="status" className="message">{message}</p>}</section>;
}
