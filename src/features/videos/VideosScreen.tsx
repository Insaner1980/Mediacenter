import {useRef,useState} from 'react';
import type {MediaItem} from '../../app/contracts';
import {errorMessage} from '../../app/backend';
import {PageHeader} from '../../ui/PageHeader';
import {LibraryFolderActions} from '../library/LibraryFolderActions';
import {Dialog} from '../../ui/Dialog';
import {useMediaLibrary} from '../library/useMediaLibrary';
import {LibraryToolbar} from '../library/LibraryToolbar';
import {LibraryPagination} from '../library/LibraryPagination';
import {FolderManager} from '../library/FolderManager';
import {MediaThumbnail} from '../library/MediaThumbnail';
import {VideoCard} from './VideoCard';
import './videos.css';
export function VideosScreen({onPlay}:{onPlay:(item:MediaItem,resume:boolean)=>Promise<void>}){
 const library=useMediaLibrary('video'),[folders,setFolders]=useState(false),[selected,setSelected]=useState<MediaItem|null>(null),[busy,setBusy]=useState(false),[error,setError]=useState(''),lock=useRef(false);
 async function play(item:MediaItem,resume:boolean){if(lock.current)return;lock.current=true;setBusy(true);setError('');try{await onPlay(item,resume);}catch(e){setError(errorMessage(e));}finally{lock.current=false;setBusy(false);}}
 return <><PageHeader title="Videos" actions={<LibraryFolderActions library={library} onManage={()=>setFolders(true)}/>}/><LibraryToolbar kind="video" {...library} onQueryChange={library.setQuery} onSourceChange={library.setSourceId} onRefresh={library.refresh}/>{(error||library.error)&&<p role="alert" className="message">{error||library.error}</p>}{library.loading&&<p role="status">Loading videos</p>}<div className="media-grid video-grid" aria-busy={library.loading}>{library.items.map(item=><VideoCard key={item.id} item={item} disabled={busy||library.loading||library.stale} onPreview={()=>setSelected(item)} onPlay={resume=>void play(item,resume)}/>)}</div>{!library.loading&&!library.error&&!library.items.length&&<div className="library-empty"><p>{library.sources.length?'No matching videos':'No videos yet'}</p>{!library.sources.length?<button className="control" onClick={()=>setFolders(true)}>Add folder</button>:library.query&&<button className="control" onClick={()=>library.setQuery('')}>Clear search</button>}</div>}<LibraryPagination library={library}/><FolderManager open={folders} kind="video" onRequestClose={()=>setFolders(false)}/><Dialog open={!!selected} title={selected?.displayName??'Video preview'} onRequestClose={()=>setSelected(null)}>{selected&&<><MediaThumbnail item={selected}/><p>{selected.displayName}</p>{selected.availability!=='available'&&<p>{selected.availability}</p>}</>}</Dialog></>;
}
