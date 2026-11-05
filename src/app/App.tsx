import { useEffect, useState } from 'react';
import { isTauri } from '@tauri-apps/api/core';
import {backend,subscribeHome,errorMessage} from './backend';
import { HomeScreen } from '../features/home/HomeScreen';
import type { HomeSnapshot, HomeAction, Screen, Theme } from '../features/home/homeModel';
import { AppShell } from '../ui/AppShell';
import {VpnSettings} from '../features/vpn/VpnSettings';
import {StreamingSession} from '../features/streaming/StreamingSession';
import '../ui/theme.css';
import {SourcePicker} from '../features/library/SourcePicker';
import {DrivesScreen} from '../features/drives/DrivesScreen';
import {OutputSettings} from '../features/outputs/OutputSettings';
const initial: HomeSnapshot = {revision:0,theme:document.documentElement.dataset.theme==='light'?'light':'dark',vpn:{revision:0,errorCode:null,state:'unknown',country:null,killSwitch:'unknown',observedAtMs:0},outputs:{revision:0,desired:{monitorId:null,audioSinkId:null},effective:{monitorId:null,audioSinkId:null},label:'Detecting outputs',state:'unavailable'},summaries:{video:'Add a video folder',audio:'Add a music folder',photo:'Add a photo folder',drives:'Checking drives',resumeProgress:null},services:[],sessions:[]};
export function App() {
  const [snapshot,setSnapshot] = useState(initial);
  const [screen,setScreen] = useState<Screen|'streaming'>('home');
  const [busy,setBusy]=useState(false);
  const [message,setMessage] = useState('');
  useEffect(()=>{
    let live=true;let dispose:(()=>void)|undefined;
    if (import.meta.env.DEV && new URL(window.location.href).searchParams.get('fixture')==='reference') {
      void import('../testing/fixtures').then(({referenceSnapshot})=>{if(live)setSnapshot({...referenceSnapshot,theme:initial.theme});});
    } else if(isTauri()) {
      void subscribeHome(s=>{if(live){setSnapshot(s);document.documentElement.dataset.theme=s.theme;localStorage.setItem('mediacenter.theme',s.theme);}}).then(stop=>{if(live)dispose=stop;else stop();}).catch(()=>{if(live)setMessage('The media services are not available yet.');});
    } else setMessage('Open the desktop application to use media services.');
    return ()=>{live=false;dispose?.();};
  },[]);
  async function action(a: HomeAction) {
    if(a.type==='navigate') { setScreen(a.screen);setMessage(''); }
    else {
      if(busy)return;
      if(!isTauri()){setMessage('Open the desktop application to launch a service.');return;}
      setBusy(true);setMessage('Opening service');
      try{await backend.openService(a.serviceId);setScreen('streaming');setMessage('');}catch(reason){setMessage(errorMessage(reason));}finally{setBusy(false);}
    }
  }
  async function theme(value: Theme) {
    if(isTauri()) {try{await backend.setTheme(value);}catch{setMessage('The theme could not be saved.');return;}}
    localStorage.setItem('mediacenter.theme',value);
    document.documentElement.dataset.theme=value;
    setSnapshot(s=>({...s,theme:value}));
  }
  if(screen==='home') return <><HomeScreen snapshot={snapshot} onAction={action}/>{message && <div role="status" className="message">{message}</div>}</>;
  if(screen==='streaming')return <AppShell snapshot={snapshot}><section className="screen"><button className="control" onClick={()=>setScreen('home')}>Back</button><StreamingSession session={snapshot.sessions.find(session=>session.kind==='streaming')} services={snapshot.services}/><OutputSettings output={snapshot.outputs} session={snapshot.sessions.find(s=>s.kind==='streaming')}/></section></AppShell>;
  return <AppShell snapshot={snapshot}><section className="screen"><button className="control" onClick={()=>setScreen('home')}>Back</button><h1>{({video:'Videos',audio:'Music',photo:'Photos',drives:'Drives & USB',settings:'VPN & Sound'})[screen]}</h1>{screen==='settings' ? <><section><h2>Appearance</h2><div className="actions"><button className="control" aria-pressed={snapshot.theme==='light'} onClick={()=>void theme('light')}>Light</button><button className="control" aria-pressed={snapshot.theme==='dark'} onClick={()=>void theme('dark')}>Dark</button></div></section><OutputSettings output={snapshot.outputs} session={snapshot.sessions.find(s=>s.kind==='streaming')}/><VpnSettings vpn={snapshot.vpn} services={snapshot.services}/></> : screen==='drives'?<DrivesScreen/>:<SourcePicker kind={screen}/>}{message && <p role="alert" className="message">{message}</p>}</section></AppShell>;
}
