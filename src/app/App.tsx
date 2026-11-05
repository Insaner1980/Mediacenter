import { useEffect, useState } from 'react';
import { isTauri } from '@tauri-apps/api/core';
import {backend,subscribeHome} from './backend';
import { HomeScreen } from '../features/home/HomeScreen';
import type { HomeSnapshot, HomeAction, Screen, Theme } from '../features/home/homeModel';
import { AppShell } from '../ui/AppShell';
import {VpnSettings} from '../features/vpn/VpnSettings';
import '../ui/theme.css';
const initial: HomeSnapshot = {revision:0,theme:document.documentElement.dataset.theme==='light'?'light':'dark',vpn:{revision:0,errorCode:null,state:'unknown',country:null,killSwitch:'unknown',observedAtMs:0},outputs:{revision:0,desired:{monitorId:null,audioSinkId:null},effective:{monitorId:null,audioSinkId:null},label:'Detecting outputs',state:'unavailable'},summaries:{video:'Add a video folder',audio:'Add a music folder',photo:'Add a photo folder',drives:'Checking drives',resumeProgress:null},services:[],sessions:[]};
export function App() {
  const [snapshot,setSnapshot] = useState(initial);
  const [screen,setScreen] = useState<Screen>('home');
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
  function action(a: HomeAction) {
    if(a.type==='navigate') { setScreen(a.screen);setMessage(''); }
    else setMessage('Service launching is not available in this build.');
  }
  async function theme(value: Theme) {
    if(isTauri()) {try{await backend.setTheme(value);}catch{setMessage('The theme could not be saved.');return;}}
    localStorage.setItem('mediacenter.theme',value);
    document.documentElement.dataset.theme=value;
    setSnapshot(s=>({...s,theme:value}));
  }
  if(screen==='home') return <><HomeScreen snapshot={snapshot} onAction={action}/>{message && <div role="status" className="message">{message}</div>}</>;
  return <AppShell snapshot={snapshot}><section className="screen"><button className="control" onClick={()=>setScreen('home')}>Back</button><h1>{({video:'Videos',audio:'Music',photo:'Photos',drives:'Drives & USB',settings:'VPN & Sound'})[screen]}</h1>{screen==='settings' ? <><section><h2>Appearance</h2><div className="actions"><button className="control" aria-pressed={snapshot.theme==='light'} onClick={()=>void theme('light')}>Light</button><button className="control" aria-pressed={snapshot.theme==='dark'} onClick={()=>void theme('dark')}>Dark</button></div></section><VpnSettings vpn={snapshot.vpn} services={snapshot.services}/></> : <p>Library controls are not available in this build.</p>}{message && <p role="alert" className="message">{message}</p>}</section></AppShell>;
}
