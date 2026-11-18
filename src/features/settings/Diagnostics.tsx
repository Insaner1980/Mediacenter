import {useState} from 'react';
import type {DiagnosticReport} from '../../app/contracts';
import {errorMessage} from '../../app/backend';
import {useAppServices} from '../../app/AppServices';
export function Diagnostics(){const {settings}=useAppServices();const [report,setReport]=useState<DiagnosticReport|null>(null),[busy,setBusy]=useState(false),[message,setMessage]=useState('');
 async function run(work:()=>Promise<void>){setBusy(true);setMessage('');try{await work();}catch(e){setMessage(errorMessage(e));}finally{setBusy(false);}}
 return <section><p>Reports exclude browser profiles, credentials, public IP addresses and media paths.</p><div className="actions"><button className="control" disabled={busy} onClick={()=>void run(async()=>setReport(await settings.preview()))}>Preview diagnostics</button>{report&&<button className="control" disabled={busy} onClick={()=>void run(async()=>{if(await settings.exportReport())setMessage('Diagnostics saved.');})}>Save diagnostics report</button>}</div>{report&&<pre className="diagnostic-preview">{JSON.stringify(report,null,2)}</pre>}{message&&<p role="status" className="message">{message}</p>}</section>;
}
