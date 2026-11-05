import {useEffect,useState} from 'react';
import {errorMessage} from '../../app/backend';
import type {MusicAction,MusicItem,MusicQueue,Playlist,PlaylistSummary,RepeatMode} from '../../app/contracts';
import {MediaBrowser} from '../library/MediaBrowser';
import {LocalPlayerControls} from '../player/LocalPlayerControls';
import {musicApi} from './musicApi';

function Tags({item}:{item:MusicItem}){
 return <><strong>{item.title}</strong>{item.artist && <p>Artist: {item.artist}</p>}{item.album && <p>Album: {item.album}</p>}<p>Availability: {item.availability}</p></>;
}
export function MusicScreen({api=musicApi}:{api?:typeof musicApi}){
 const [queue,setQueue]=useState<MusicQueue|null>(null);
 const [playlists,setPlaylists]=useState<PlaylistSummary[]>([]);
 const [draft,setDraft]=useState<Playlist|null>(null);
 const [busy,setBusy]=useState(false);
 const [error,setError]=useState('');
 const [notice,setNotice]=useState('');
 const [page,setPage]=useState(0);
 const [draftPage,setDraftPage]=useState(0);
 useEffect(()=>{
  let live=true;let dispose:(()=>void)|undefined;
  const update=(value:MusicQueue)=>{if(live)setQueue(current=>!current||value.revision>=current.revision?value:current);};
  void api.subscribe(update).then(async stop=>{if(!live){stop();return;}dispose=stop;update(await api.queue());}).catch(reason=>{if(live)setError(errorMessage(reason));});
  void api.playlists().then(value=>{if(live)setPlaylists(value);}).catch(reason=>{if(live)setError(errorMessage(reason));});
  return()=>{live=false;dispose?.();};
 },[api]);
 function accept(value:MusicQueue){setQueue(current=>!current||value.revision>=current.revision?value:current);}
 async function run(work:()=>Promise<unknown>){
  if(busy)return;setBusy(true);setError('');setNotice('');
  try{await work();}catch(reason){setError(errorMessage(reason));try{accept(await api.queue());}catch{/* Preserve the original failure. */}}finally{setBusy(false);}
 }
 async function control(action:MusicAction){if(queue)accept(await api.control(queue,action));}
 function moveDraft(from:number,to:number){setDraft(current=>{if(!current)return current;const items=[...current.items];const [item]=items.splice(from,1);items.splice(to,0,item);return {...current,items};});}
 const queuePage=Math.min(page,Math.max(0,Math.ceil((queue?.items.length??0)/50)-1));
 const editorPage=Math.min(draftPage,Math.max(0,Math.ceil((draft?.items.length??0)/50)-1));
 return <>
  <p>Music keeps playing while you browse other screens. Playlist edits are saved only when you select Save playlist.</p>
  {error && <p className="message" role="alert">{error}</p>}
  {notice && <p role="status">{notice}</p>}
  <LocalPlayerControls/>
  <section><h2>Music queue</h2>
   {!queue && <p>Loading music queue</p>}
   {queue && <>
    <p>{queue.items.length} tracks{queue.currentIndex!==null?` | Current track ${queue.currentIndex+1}`:''}</p>
    {queue.message && <p role="status">{queue.message}</p>}
    {!queue.items.length && <p>Add music from the library or play a saved playlist.</p>}
    <div className="actions">
     <button className="control" disabled={busy||!queue.items.length} onClick={()=>void run(()=>control({type:'previous'}))}>Previous track</button>
     <button className="control" disabled={busy||!queue.items.length} onClick={()=>void run(()=>control({type:'next'}))}>Next track</button>
     <button className="control" disabled={busy} aria-pressed={queue.shuffle} onClick={()=>void run(()=>control({type:'setShuffle',enabled:!queue.shuffle}))}>Shuffle {queue.shuffle?'on':'off'}</button>
     <button className="control" disabled={busy} onClick={()=>void run(async()=>accept(await api.queue()))}>Refresh queue</button>
    </div>
    <label className="field">Repeat<select value={queue.repeat} disabled={busy} onChange={e=>void run(()=>control({type:'setRepeat',mode:e.target.value as RepeatMode}))}><option value="off">Off</option><option value="one">One track</option><option value="all">All tracks</option></select></label>
    <div className="media-list">{queue.items.slice(queuePage*50,queuePage*50+50).map((entry,offset)=>{
     const index=queuePage*50+offset;
     return <article key={entry.entryId}><p>Track {index+1}{queue.currentIndex===index?' | Selected':''}</p><Tags item={entry.item}/><div className="actions">
      <button className="control" aria-label={`Play queue entry ${index+1}`} disabled={busy||entry.item.availability!=='available'} onClick={()=>void run(()=>control({type:'playAt',index}))}>Play track</button>
      <button className="control" aria-label={`Move queue entry ${index+1} up`} disabled={busy||index===0} onClick={()=>void run(()=>control({type:'moveItem',from:index,to:index-1}))}>Move up</button>
      <button className="control" aria-label={`Move queue entry ${index+1} down`} disabled={busy||index===queue.items.length-1} onClick={()=>void run(()=>control({type:'moveItem',from:index,to:index+1}))}>Move down</button>
      <button className="control" aria-label={`Remove queue entry ${index+1}`} disabled={busy} onClick={()=>void run(()=>control({type:'removeAt',index}))}>Remove from queue</button>
     </div></article>;
    })}</div>
    {queue.items.length>50 && <div className="actions"><button className="control" disabled={queuePage===0} onClick={()=>setPage(queuePage-1)}>Previous queue page</button><button className="control" disabled={(queuePage+1)*50>=queue.items.length} onClick={()=>setPage(queuePage+1)}>Next queue page</button></div>}
    <button className="control" disabled={busy} onClick={()=>{setDraft({id:'',name:'',items:queue.items.map(e=>e.item)});setDraftPage(0);}}>New playlist from queue</button>
   </>}
  </section>
  <section><h2>Saved playlists</h2>
   {!playlists.length && <p>No saved playlists.</p>}
   <div className="media-list">{playlists.map(playlist=><article key={playlist.id}><strong>{playlist.name}</strong><p>{playlist.itemCount} tracks</p><div className="actions">
    <button className="control" aria-label={`Play ${playlist.name}`} disabled={busy||playlist.itemCount===0} onClick={()=>void run(async()=>{await api.playPlaylist(playlist.id);accept(await api.queue());})}>Play playlist</button>
    <button className="control" aria-label={`Edit ${playlist.name}`} disabled={busy} onClick={()=>void run(async()=>{setDraft(await api.playlist(playlist.id));setDraftPage(0);})}>Edit playlist</button>
    <button className="control" aria-label={`Delete ${playlist.name}`} disabled={busy} onClick={()=>void run(async()=>{await api.remove(playlist.id);setPlaylists(await api.playlists());if(draft?.id===playlist.id)setDraft(null);setNotice('Playlist removed. Music files were kept.');})}>Delete playlist</button>
   </div></article>)}</div>
  </section>
  {draft && <section><h2>Playlist editor</h2>
   <label className="field">Playlist name<input value={draft.name} disabled={busy} onChange={e=>setDraft({...draft,name:e.target.value})}/></label>
   <p>{draft.items.length} tracks. Removing a track here changes this draft only.</p>
   <div className="media-list">{draft.items.slice(editorPage*50,editorPage*50+50).map((item,offset)=>{const index=editorPage*50+offset;return <article key={`${index}:${item.mediaId}`}><p>Playlist track {index+1}</p><Tags item={item}/><div className="actions">
    <button className="control" aria-label={`Move playlist entry ${index+1} up`} disabled={busy||index===0} onClick={()=>moveDraft(index,index-1)}>Move up</button>
    <button className="control" aria-label={`Move playlist entry ${index+1} down`} disabled={busy||index===draft.items.length-1} onClick={()=>moveDraft(index,index+1)}>Move down</button>
    <button className="control" aria-label={`Remove playlist entry ${index+1}`} disabled={busy} onClick={()=>setDraft({...draft,items:draft.items.filter((_,i)=>i!==index)})}>Remove from playlist</button>
   </div></article>;})}</div>
   {draft.items.length>50 && <div className="actions"><button className="control" disabled={editorPage===0} onClick={()=>setDraftPage(editorPage-1)}>Previous playlist page</button><button className="control" disabled={(editorPage+1)*50>=draft.items.length} onClick={()=>setDraftPage(editorPage+1)}>Next playlist page</button></div>}
   <div className="actions">
    <button className="control" disabled={busy||!draft.name.trim()||Array.from(draft.name).length>200||draft.items.length>10000} onClick={()=>void run(async()=>{const id=await api.save(draft.id||null,draft.name,draft.items.map(item=>item.mediaId));setDraft({...draft,id});setPlaylists(await api.playlists());setNotice('Playlist saved.');})}>Save playlist</button>
    <button className="control" disabled={busy} onClick={()=>setDraft(null)}>Close editor</button>
    <button className="control" disabled={busy||!queue?.items.length||draft.items.length+(queue?.items.length??0)>10000} onClick={()=>{if(queue)setDraft({...draft,items:[...draft.items,...queue.items.map(e=>e.item)]});}}>Append queue to draft</button>
   </div>
  </section>}
  <section><h2>Music library</h2><MediaBrowser kind="audio" onPlay={item=>void run(async()=>{await api.play([item.id]);accept(await api.queue());})} onEnqueue={item=>void run(()=>control({type:'enqueue',mediaIds:[item.id]}))}/></section>
 </>;
}
