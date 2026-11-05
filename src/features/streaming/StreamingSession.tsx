import {useState} from 'react';
import type {ManagedSession,ServiceDefinition} from '../../app/contracts';
import {backend,errorMessage} from '../../app/backend';
export function StreamingSession({session,services}:{session:ManagedSession|undefined;services:ServiceDefinition[]}) {
  const [busy,setBusy]=useState(false);const [error,setError]=useState('');
  async function run(work:()=>Promise<unknown>){if(busy)return;setBusy(true);setError('');try{await work();}catch(reason){setError(errorMessage(reason));}finally{setBusy(false);}}
  if(!session)return <p>No managed service is open.</p>;
  return <section><h1>{services.find(service=>service.id===session.serviceId)?.name ?? 'Streaming service'}</h1><p>Service window: {session.phase}</p><p>Choose content, subtitles and playback controls in the service's browser window.</p><div className="actions"><button className="control" disabled={busy} onClick={()=>void run(()=>session.serviceId ? backend.openService(session.serviceId) : backend.focusSession(session.id))}>Open service window</button><button className="control" disabled={busy} onClick={()=>void run(()=>backend.closeSession(session.id))}>Close service</button></div>{busy && <p role="status">Updating service window</p>}{error && <p className="message" role="alert">{error}</p>}</section>;
}
