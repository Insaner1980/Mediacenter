import {useRef,useState} from 'react';
import type {MediaItem} from '../../app/contracts';
import {errorMessage} from '../../app/backend';
import {PageHeader} from '../../ui/PageHeader';
import {LibraryFolderActions} from '../library/LibraryFolderActions';
import {useMediaLibrary} from '../library/useMediaLibrary';
import {LibraryToolbar} from '../library/LibraryToolbar';
import {LibraryPagination} from '../library/LibraryPagination';
import {VideoCard} from './VideoCard';
import './videos.css';
export function VideosScreen({onPlay}:{onPlay:(item:MediaItem,resume:boolean)=>Promise<void>}){
 const library=useMediaLibrary('video'),[busy,setBusy]=useState(false),[error,setError]=useState(''),lock=useRef(false);
 async function play(item:MediaItem,resume:boolean){if(lock.current)return;lock.current=true;setBusy(true);setError('');try{await onPlay(item,resume);}catch(e){setError(errorMessage(e));}finally{lock.current=false;setBusy(false);}}
 return <><PageHeader title="Videos" actions={<LibraryFolderActions library={library}/>}/><LibraryToolbar kind="video" {...library} onQueryChange={library.setQuery} onSourceChange={library.setSourceId} onRefresh={library.refresh}/>{(error||library.error)&&<p role="alert" className="message">{library.error||error}</p>}{library.loading&&<p role="status">Loading videos</p>}<div data-remote-section="video-items" data-remote-layout="grid" className="media-grid video-grid" aria-busy={library.loading}>{library.items.map(item=><VideoCard key={item.id} item={item} disabled={busy||library.loading||library.stale} onPlay={resume=>void play(item,resume)}/>)}</div>{!library.loading&&!library.error&&!library.items.length&&<div className="library-empty"><p>{library.sources.length?'No matching videos':'No videos yet'}</p>{library.query&&<button className="control" onClick={()=>library.setQuery('')}>Clear search</button>}</div>}<LibraryPagination library={library}/></>;
}
