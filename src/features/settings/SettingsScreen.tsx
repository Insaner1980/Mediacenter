import {useEffect,useRef,useState} from 'react';
import type {HomeSnapshot,Theme} from '../../app/contracts';
import type {SettingsSection} from '../../app/navigation';
import {PageHeader} from '../../ui/PageHeader';
import {Sheet} from '../../ui/Sheet';
import {OutputSettings} from '../outputs/OutputSettings';
import {VpnSettings} from '../vpn/VpnSettings';
import {Diagnostics} from './Diagnostics';
import './settings.css';
export function SettingsScreen({snapshot,onThemeChange,initialSection}:{snapshot:HomeSnapshot;onThemeChange:(theme:Theme)=>Promise<void>;initialSection:SettingsSection|null}){const [diagnostics,setDiagnostics]=useState(false),root=useRef<HTMLDivElement>(null);useEffect(()=>{if(initialSection==='diagnostics')setDiagnostics(true);else if(initialSection)root.current?.querySelector<HTMLElement>(`#settings-${initialSection}`)?.focus();},[initialSection]);return <div ref={root} className="settings-screen"><PageHeader title="Settings"/><div className="settings-columns"><div><div id="settings-outputs" tabIndex={-1}><OutputSettings output={snapshot.outputs} session={snapshot.sessions.find(s=>s.kind==='streaming'||s.kind.startsWith('local-'))}/></div><section id="settings-appearance" tabIndex={-1} className="appearance"><h2>Appearance</h2><div className="segmented"><button className="control" aria-pressed={snapshot.theme==='light'} onClick={()=>void onThemeChange('light')}>Light</button><button className="control" aria-pressed={snapshot.theme==='dark'} onClick={()=>void onThemeChange('dark')}>Dark</button></div></section></div><div><div id="settings-vpn" tabIndex={-1}><VpnSettings vpn={snapshot.vpn} services={snapshot.services}/></div><button className="disclosure-button diagnostics-entry" onClick={()=>setDiagnostics(true)}>Diagnostics<span aria-hidden="true">›</span></button></div></div><Sheet open={diagnostics} title="Diagnostics" onRequestClose={()=>setDiagnostics(false)}><Diagnostics/></Sheet></div>;}
