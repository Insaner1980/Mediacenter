import type {MainScreen} from '../../app/navigation';
import {Icon} from '../../ui/Icons';
const rows={video:['video','Videos'],audio:['music','Music'],photo:['photos','Photos'],drives:['drive','Drives & USB']} as const;
export function PersonalMediaRow({kind,onOpen}:{kind:keyof typeof rows;onOpen:(screen:MainScreen)=>void}){const [icon,title]=rows[kind];return <button className="media-row" type="button" onClick={()=>onOpen(kind)}><span className="media-icon"><Icon name={icon}/></span><span className="media-title">{title}</span></button>;}
