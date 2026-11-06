import type {MediaKind,MediaSource} from '../../app/contracts';
import {Icon} from '../../ui/Icons';
import {IconButton} from '../../ui/IconButton';
import './library.css';
export function LibraryToolbar({kind,query,sourceId,sources,onQueryChange,onSourceChange,onRefresh,scanning,cancelScan}:{kind:MediaKind;query:string;sourceId:string|null;sources:MediaSource[];onQueryChange:(s:string)=>void;onSourceChange:(s:string|null)=>void;onRefresh:()=>void;scanning?:boolean;cancelScan?:()=>void}){
 const label=`Search ${kind==='audio'?'music':kind==='photo'?'photos':'videos'}`;
 return <><div className="library-toolbar"><label className="search-field"><Icon name="search"/><input aria-label={label} placeholder={label} value={query} maxLength={200} onChange={e=>onQueryChange(e.target.value)}/></label><select aria-label="Source" value={sourceId??''} onChange={e=>onSourceChange(e.target.value||null)}><option value="">All folders</option>{sources.filter(s=>s.kinds.includes(kind)).map(s=><option value={s.id} key={s.id}>{s.label}{s.availability==='available'?'':` (${s.availability})`}</option>)}{sourceId&&!sources.some(s=>s.id===sourceId)&&<option value={sourceId}>Folder unavailable</option>}</select><IconButton label="Refresh library" icon="refresh" onClick={onRefresh}/></div>{scanning&&<div className="library-scan"><span role="status">Updating folder</span><button className="control" onClick={cancelScan}>Cancel scan</button></div>}</>;
}
