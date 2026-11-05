import type { Screen } from './homeModel';
import { Icon } from '../../ui/Icons';
const rows = {video:['video','Videos'],audio:['music','Music'],photo:['photos','Photos'],drives:['drive','Drives & USB'],settings:['settings','VPN & Sound']} as const;
export function PersonalMediaRow({kind,summary,onOpen,progress=null}: {kind:Exclude<Screen,'home'>;summary:string;onOpen:(screen:Screen)=>void;progress?:number|null}) {
  const [icon,title] = rows[kind];
  return <button className="media-row" type="button" onClick={()=>onOpen(kind)}>
    <span className="media-icon"><Icon name={icon}/></span><span className="media-copy"><span className="media-title">{title}</span><span className="media-detail">{summary.startsWith('Resume ') ? <><strong>Resume</strong> {summary.slice(7)}</> : summary}</span>{progress !== null && <span className="progress" aria-hidden="true"><span style={{width:`${Math.max(0,Math.min(1,progress))*100}%`}}/></span>}</span>
  </button>;
}
