import {RemoteNavigationOrigin} from './navigation';
import {RemoteNavigationBridge} from '../features/remote/RemoteNavigationBridge';
import { useEffect, useState, useRef, useCallback } from 'react';
import {SlideshowProvider} from '../features/photos/SlideshowProvider';
import {SlideshowStrip} from '../features/photos/SlideshowControls';
import {SourceManagerProvider} from '../features/library/SourceManagerProvider';
import {PlaybackProvider} from '../features/player/PlaybackProvider';
import {NavigationGuardProvider,type NavigationGuard} from './NavigationGuard';
import type {MainScreen,SettingsSection,MusicTab,NavigationState} from './navigation';
import {AppServicesProvider,useAppServices,productionServices,type AppServices} from './AppServices';
import { isTauri } from '@tauri-apps/api/core';
import {errorMessage} from './backend';
import { HomeScreen } from '../features/home/HomeScreen';
import type { HomeSnapshot, HomeAction, Screen, Theme } from '../features/home/homeModel';
import { AppShell } from '../ui/AppShell';
import {SettingsScreen} from '../features/settings/SettingsScreen';
import {StreamingSession} from '../features/streaming/StreamingSession';
import '../ui/theme.css';
import {getCurrentWindow} from '@tauri-apps/api/window';
import {MusicScreen} from '../features/music/MusicScreen';
import {PhotosScreen} from '../features/photos/PhotosScreen';
import {PresentationScreen} from '../features/photos/PresentationScreen';
import {ExitDialog} from '../features/settings/ExitDialog';
import type {MediaItem} from './contracts';
import {VideosScreen} from '../features/videos/VideosScreen';
import {DrivesScreen} from '../features/drives/DrivesScreen';
import {OutputSettings} from '../features/outputs/OutputSettings';
const initial: HomeSnapshot = {revision:0,theme:document.documentElement.dataset.theme==='light'?'light':'dark',vpn:{revision:0,errorCode:null,state:'unknown',country:null,killSwitch:'unknown',observedAtMs:0},outputs:{revision:0,desired:{monitorId:null,audioSinkId:null},effective:{monitorId:null,audioSinkId:null},label:'Detecting outputs',state:'unavailable'},summaries:{video:'Add a video folder',audio:'Add a music folder',photo:'Add a photo folder',drives:'Checking drives',resumeProgress:null},services:[],sessions:[]};
function applyTheme(value: Theme) {
  document.documentElement.dataset.theme = value;
  try { localStorage.setItem('mediacenter.theme', value); } catch { /* The cache only affects startup. */ }
}
function MainContent({initialNavigation}:{initialNavigation?:Partial<NavigationState>}={}) {
  const {backend,subscribeHome,canUseMedia}=useAppServices();
  const [snapshot,setSnapshot] = useState(initial);
  const [screen,setScreen] = useState<Screen|'streaming'>(initialNavigation?.route??'home');
  const [remoteNavigation,setRemoteNavigation]=useState(false);
  const routeHistory=useRef<Array<Screen|'streaming'>>([]);
  const [settingsSection,setSettingsSection]=useState<SettingsSection|null>(initialNavigation?.settingsSection??null);
  const [musicTab,setMusicTab]=useState<MusicTab>(initialNavigation?.musicTab??'library');
  const guard=useRef<NavigationGuard|null>(null);
  const register=useCallback((value:NavigationGuard|null)=>{guard.current=value;},[]);
  function navigate(route:Screen|'streaming',remote=false){const go=()=>{if(route!==screen)routeHistory.current.push(screen);setRemoteNavigation(remote);setScreen(route);};if(guard.current)guard.current(go);else go();}
  function remoteBack(){const go=()=>{setRemoteNavigation(true);setScreen(routeHistory.current.pop()??'home');};if(guard.current)guard.current(go);else go();}
  const [busy,setBusy]=useState(false);
  const [message,setMessage] = useState('');
  useEffect(()=>{
    let live=true;let dispose:(()=>void)|undefined;
    if(canUseMedia) {
      void subscribeHome(s=>{if(live){setSnapshot(s);applyTheme(s.theme);}}).then(stop=>{if(live)dispose=stop;else stop();}).catch(()=>{if(live)setMessage('The media services are not available yet.');});
    } else setMessage('Open the desktop application to use media services.');
    return ()=>{live=false;dispose?.();};
  },[subscribeHome,canUseMedia]);
  async function action(a: HomeAction) {
    if(a.type==='navigate') { navigate(a.screen);setMessage(''); }
    else {
      if(busy)return;
      if(!canUseMedia){setMessage('Open the desktop application to launch a service.');return;}
      if(snapshot.sessions.some(s=>s.kind.startsWith('local-'))){setMessage('Close the player window before opening a service.');return;}
      setBusy(true);setMessage('Opening service');
      try{await backend.openService(a.serviceId);setMessage(a.serviceId==='hbo_max'?'Continue in Chrome for HBO Max. Choose its display and sound in your system settings.':'');}catch(reason){setMessage(errorMessage(reason));}finally{setBusy(false);}
    }
  }
  async function play(item:MediaItem,resume:boolean){if(busy)return;setBusy(true);setMessage('Opening local media');try{await backend.playMedia(item.id,resume);setMessage('');}catch(reason){setMessage(errorMessage(reason));}finally{setBusy(false);}}
  async function theme(value: Theme) {
    if(canUseMedia) {try{await backend.setTheme(value);}catch{setMessage('The theme could not be saved.');return;}}
    applyTheme(value);
    setSnapshot(s=>({...s,theme:value}));
  }
  return <RemoteNavigationOrigin.Provider value={remoteNavigation}><NavigationGuardProvider register={register}><RemoteNavigationBridge route={screen} navigate={route=>{if(route==='settings')setSettingsSection(null);navigate(route,true);}} back={remoteBack}/><AppShell snapshot={snapshot} route={screen} onNavigate={navigate} onOpenSettings={section=>{setSettingsSection(section??null);navigate('settings');}} player={<SlideshowStrip/>} >
    {screen==='home'?<HomeScreen snapshot={snapshot} onAction={action} message={message} onPlaybackControls={()=>{setScreen('streaming');setMessage('');}}/>:
    <section className="screen">{screen==='streaming'?<><StreamingSession session={snapshot.sessions.find(s=>s.kind==='streaming')} services={snapshot.services}/><OutputSettings output={snapshot.outputs} session={snapshot.sessions.find(s=>s.kind==='streaming')}/></>:<>{screen==='settings'?<SettingsScreen snapshot={snapshot} onThemeChange={theme} initialSection={settingsSection}/>:screen==='drives'?<DrivesScreen/>:screen==='audio'?<MusicScreen tab={musicTab} onTabChange={setMusicTab}/>:screen==='photo'?<PhotosScreen/>:<VideosScreen onPlay={play}/>}</>}{message&&<p role="alert" className="message">{message}</p>}</section>}
  </AppShell></NavigationGuardProvider></RemoteNavigationOrigin.Provider>;
}

export function MainApp(props:{initialNavigation?:Partial<NavigationState>}){return <SourceManagerProvider><PlaybackProvider><SlideshowProvider><MainContent {...props}/></SlideshowProvider></PlaybackProvider></SourceManagerProvider>;}
function MainWindow(){
 const [runtime,setRuntime]=useState<{services:AppServices;navigation?:Partial<NavigationState>}|null>(()=>{
  const params=new URL(window.location.href).searchParams;
  return import.meta.env.DEV&&['reference','redesign'].includes(params.get('fixture')??'')?null:{services:productionServices};
 });
 useEffect(()=>{if(runtime)return;let live=true;const params=new URL(window.location.href).searchParams;const requested=params.get('screen')??'home';const route:MainScreen=['home','video','audio','photo','drives','settings'].includes(requested)?requested as MainScreen:'home';const theme:Theme=params.get('theme')==='dark'?'dark':params.get('theme')==='light'?'light':document.documentElement.dataset.theme==='light'?'light':'dark';
  if(import.meta.env.DEV)void import('../testing/fixtureServices').then(({createFixtureServices})=>{if(live)setRuntime({services:createFixtureServices({screen:route,state:params.get('state')??'populated',theme}),navigation:{route}});});return()=>{live=false;};
 },[runtime]);
 if(!runtime)return <p role="status">Loading preview</p>;
 return <AppServicesProvider services={runtime.services}><MainApp initialNavigation={runtime.navigation}/>{runtime.services===productionServices&&<ExitDialog/>}</AppServicesProvider>;
}
export function App(){if(isTauri()&&getCurrentWindow().label==='presentation')return <PresentationScreen/>;return <MainWindow/>;}
