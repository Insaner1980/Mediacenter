import {useState,type ReactNode} from 'react';
import {OutputPanel} from '../features/outputs/OutputPanel';
import type {HomeSnapshot} from '../features/home/homeModel';
import type {AppRoute,MainScreen,SettingsSection} from '../app/navigation';
import {Icon} from './Icons';
import {Sidebar} from './Sidebar';
import {useVpnObservation} from '../features/vpn/useVpnObservation';
import './shell.css';
export interface AppShellProps {snapshot:HomeSnapshot;route:AppRoute;onNavigate:(screen:MainScreen)=>void;onOpenSettings:(section?:SettingsSection)=>void;children:ReactNode;player?:ReactNode}
export function AppShell({snapshot,route,onNavigate,onOpenSettings,children,player}:AppShellProps){
 const vpn=useVpnObservation(snapshot.vpn),connected=vpn.state==='connected';
 const labels:Record<string,string>={disconnected:'VPN disconnected',unknown:'VPN status unknown',unavailable:'NordVPN unavailable',connecting:'VPN connecting',disconnecting:'VPN disconnecting',unauthenticated:'Sign in to NordVPN',error:'VPN status unavailable'};
 const [outputOpen,setOutputOpen]=useState(false);
 return <div className={`app ${route==='home'?'app-home':'app-inner'}`}><header className="app-header"><div className="brand"><span className="brand-mark" aria-hidden="true"><svg viewBox="0 0 16 16"><path d="M4 2 14 8 4 14z"/></svg></span>mediacenter</div><div className="connections"><button className="connection connection-vpn" data-connected={connected} onClick={()=>onOpenSettings('vpn')}><Icon name="shield"/><span>{connected?<>VPN: <strong>{vpn.country??'Country unavailable'}</strong></>:labels[vpn.state]??'VPN status unknown'}</span></button><button className="connection" aria-haspopup="dialog" aria-expanded={outputOpen} onClick={()=>setOutputOpen(true)}><Icon name="display"/><span>Picture &amp; sound</span><Icon name="chevron"/></button>{route==='home'&&<button className="connection" onClick={()=>onOpenSettings()}><Icon name="settings"/><span>Settings</span></button>}</div></header><div className="workspace">{route!=='home'&&<Sidebar route={route} onNavigate={onNavigate}/>}<main className="main-content">{children}</main></div>{player&&<div className="persistent-controls">{player}</div>}<OutputPanel open={outputOpen} onRequestClose={()=>setOutputOpen(false)} snapshot={snapshot}/></div>;
}
