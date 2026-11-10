import {remoteRouteFor} from '../app/navigation';
import type {AppRoute,MainScreen} from '../app/navigation';
import {Icon} from './Icons';
const entries:[MainScreen,string,string][]=[['home','Home','home'],['video','Videos','video'],['audio','Music','music'],['photo','Photos','photos'],['drives','Drives & USB','drive'],['settings','Settings','settings']];
export function Sidebar({route,onNavigate}:{route:AppRoute;onNavigate:(route:MainScreen)=>void}){return <nav className="sidebar" aria-label="Main navigation">{entries.map(([id,label,icon])=><button data-remote-route={id==='drives'?undefined:remoteRouteFor[id]} data-remote-id={id==='drives'?undefined:`sidebar-${id}`} data-remote-group="sidebar" data-remote-local="navigation" key={id} className={`sidebar-link ${id==='drives'?'sidebar-divider':''}`} aria-current={route===id?'page':undefined} onClick={()=>onNavigate(id)}><Icon name={icon}/><span>{label}</span></button>)}</nav>;}
