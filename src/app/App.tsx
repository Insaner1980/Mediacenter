import { useEffect, useState } from 'react';
import type {NavigationState} from './navigation';
import {AppServicesProvider,useAppServices} from './AppServices';
import { isTauri } from '@tauri-apps/api/core';
import {errorMessage} from './backend';
import { HomeScreen } from '../features/home/HomeScreen';
import type { HomeSnapshot, HomeAction, Screen, Theme } from '../features/home/homeModel';
import { AppShell } from '../ui/AppShell';
import {VpnSettings} from '../features/vpn/VpnSettings';
import {StreamingSession} from '../features/streaming/StreamingSession';
import '../ui/theme.css';
import {Diagnostics} from '../features/settings/Diagnostics';
import {getCurrentWindow} from '@tauri-apps/api/window';
import {MusicScreen} from '../features/music/MusicScreen';
import {PhotosScreen} from '../features/photos/PhotosScreen';
import {PresentationScreen} from '../features/photos/PresentationScreen';
import {ExitDialog} from '../features/settings/ExitDialog';
import {LocalPlayerControls} from '../features/player/LocalPlayerControls';
import type {MediaItem} from './contracts';
import {MediaBrowser} from '../features/library/MediaBrowser';
import {DrivesScreen} from '../features/drives/DrivesScreen';
import {OutputSettings} from '../features/outputs/OutputSettings';
const initial: HomeSnapshot = {revision:0,theme:document.documentElement.dataset.theme==='light'?'light':'dark',vpn:{revision:0,errorCode:null,state:'unknown',country:null,killSwitch:'unknown',observedAtMs:0},outputs:{revision:0,desired:{monitorId:null,audioSinkId:null},effective:{monitorId:null,audioSinkId:null},label:'Detecting outputs',state:'unavailable'},summaries:{video:'Add a video folder',audio:'Add a music folder',photo:'Add a photo folder',drives:'Checking drives',resumeProgress:null},services:[],sessions:[]};
export function MainApp({initialNavigation}:{initialNavigation?:Partial<NavigationState>}={}) {
  const {backend,subscribeHome,canUseMedia}=useAppServices();
  const [snapshot,setSnapshot] = useState(initial);
  const [screen,setScreen] = useState<Screen|'streaming'|'player'>(initialNavigation?.route??'home');
  const [busy,setBusy]=useState(false);
  const [message,setMessage] = useState('');
  useEffect(()=>{
    let live=true;let dispose:(()=>void)|undefined;
    if (import.meta.env.DEV && new URL(window.location.href).searchParams.get('fixture')==='reference') {
      void import('../testing/fixtures').then(({referenceSnapshot})=>{if(live)setSnapshot({...referenceSnapshot,theme:initial.theme});});
    } else if(canUseMedia) {
      void subscribeHome(s=>{if(live){setSnapshot(s);document.documentElement.dataset.theme=s.theme;localStorage.setItem('mediacenter.theme',s.theme);}}).then(stop=>{if(live)dispose=stop;else stop();}).catch(()=>{if(live)setMessage('The media services are not available yet.');});
    } else setMessage('Open the desktop application to use media services.');
    return ()=>{live=false;dispose?.();};
  },[subscribeHome,canUseMedia]);
  async function action(a: HomeAction) {
    if(a.type==='navigate') { setScreen(a.screen);setMessage(''); }
    else {
      if(busy)return;
      if(!canUseMedia){setMessage('Open the desktop application to launch a service.');return;}
      if(snapshot.sessions.some(s=>s.kind.startsWith('local-'))){setScreen('player');setMessage('Stop local playback before opening a service.');return;}
      setBusy(true);setMessage('Opening service');
      try{await backend.openService(a.serviceId);setMessage(a.serviceId==='hbo_max'?'Continue in Chrome for HBO Max. Choose its display and sound in your system settings.':'');}catch(reason){setMessage(errorMessage(reason));}finally{setBusy(false);}
    }
  }
  async function play(item:MediaItem,resume:boolean){if(busy)return;setBusy(true);setMessage('Opening local media');try{await backend.playMedia(item.id,resume);setScreen('player');setMessage('');}catch(reason){setMessage(errorMessage(reason));}finally{setBusy(false);}}
  async function theme(value: Theme) {
    if(canUseMedia) {try{await backend.setTheme(value);}catch{setMessage('The theme could not be saved.');return;}}
    localStorage.setItem('mediacenter.theme',value);
    document.documentElement.dataset.theme=value;
    setSnapshot(s=>({...s,theme:value}));
  }
  return <AppShell snapshot={snapshot} route={screen} onNavigate={setScreen} onOpenSettings={()=>setScreen('settings')}>
    {screen==='home'?<HomeScreen snapshot={snapshot} onAction={action} message={message} onPlaybackControls={()=>{setScreen('streaming');setMessage('');}}/>:
    <section className="screen">{screen==='player'?<><LocalPlayerControls/><OutputSettings output={snapshot.outputs} session={snapshot.sessions.find(s=>s.kind.startsWith('local-'))}/></>:screen==='streaming'?<><StreamingSession session={snapshot.sessions.find(s=>s.kind==='streaming')} services={snapshot.services}/><OutputSettings output={snapshot.outputs} session={snapshot.sessions.find(s=>s.kind==='streaming')}/></>:<><h1>{({video:'Videos',audio:'Music',photo:'Photos',drives:'Drives & USB',settings:'Settings'})[screen]}</h1>{screen==='settings'?<><section><h2>Appearance</h2><div className="actions"><button className="control" aria-pressed={snapshot.theme==='light'} onClick={()=>void theme('light')}>Light</button><button className="control" aria-pressed={snapshot.theme==='dark'} onClick={()=>void theme('dark')}>Dark</button></div></section><OutputSettings output={snapshot.outputs} session={snapshot.sessions.find(s=>s.kind==='streaming'||s.kind.startsWith('local-'))}/><VpnSettings vpn={snapshot.vpn} services={snapshot.services}/><Diagnostics/></>:screen==='drives'?<DrivesScreen/>:screen==='audio'?<MusicScreen/>:screen==='photo'?<PhotosScreen/>:<MediaBrowser kind="video" onPlay={(item,resume)=>void play(item,resume)}/>}</>}{message&&<p role="alert" className="message">{message}</p>}</section>}
  </AppShell>;
}

export function App(){if(isTauri()&&getCurrentWindow().label==='presentation')return <PresentationScreen/>;return <AppServicesProvider><MainApp/><ExitDialog/></AppServicesProvider>;}
