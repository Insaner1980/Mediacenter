import {useState} from 'react';
import type {MediaLibraryState} from '../library/useMediaLibrary';
import {LibraryToolbar} from '../library/LibraryToolbar';
import {LibraryPagination} from '../library/LibraryPagination';
import {useAppServices} from '../../app/AppServices';
import {usePlayback} from '../player/PlaybackProvider';
import {IconButton} from '../../ui/IconButton';
import {Icon} from '../../ui/Icons';
import {formatTime} from '../player/PlayerBar';
import {errorMessage} from '../../app/backend';
export function MusicLibrary({library,onAddFolder}:{library:MediaLibraryState;onAddFolder:()=>void}){
 const {music}=useAppServices(),{player,musicAction,refresh}=usePlayback();const [busy,setBusy]=useState(false),[error,setError]=useState('');
 async function play(id:string){if(busy)return;setBusy(true);try{await music.play([id]);await refresh();}catch(e){setError(errorMessage(e));}finally{setBusy(false);}}
 return <><LibraryToolbar kind="audio" {...library} onQueryChange={library.setQuery} onSourceChange={library.setSourceId} onRefresh={library.refresh}/>{(library.error||error)&&<p role="alert">{library.error||error}</p>}{library.loading&&<p role="status">Loading music</p>}<table className="music-table" aria-busy={library.loading}><thead><tr><th>Track</th><th>Source</th><th>Duration</th><th><span className="sr-only">Actions</span></th></tr></thead><tbody>{library.items.map(item=><tr key={item.id} data-current={player?.mediaId===item.id}><td><span className="track-name">{player?.mediaId===item.id?<Icon name="equalizer"/>:<span className="track-indent"/>}<span title={item.displayName}>{item.displayName}{item.availability!=='available'&&<small>{item.availability}</small>}</span></span></td><td>{library.sources.find(s=>s.id===item.sourceId)?.label??'Folder unavailable'}</td><td className="mono">{formatTime(item.durationMs===null?null:item.durationMs/1000)}</td><td><div className="track-actions"><IconButton label={`Add ${item.displayName} to queue`} icon="plus" disabled={library.loading||library.stale||busy||item.availability!=='available'} onClick={()=>void musicAction({type:'enqueue',mediaIds:[item.id]})}/><IconButton label={`Play ${item.displayName}`} icon="play" disabled={library.loading||library.stale||busy||item.availability!=='available'} onClick={()=>void play(item.id)}/></div></td></tr>)}</tbody></table>{!library.loading&&!library.error&&!library.items.length&&<div className="library-empty"><p>{library.sources.length?'No matching music':'No music yet'}</p>{!library.sources.length?<button className="control" onClick={onAddFolder}>Add folder</button>:library.query&&<button className="control" onClick={()=>library.setQuery('')}>Clear search</button>}</div>}<LibraryPagination library={library}/></>;
}
