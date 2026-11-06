import {useEffect,useRef,useState} from 'react';
import {AppServicesProvider,useAppServices} from '../../app/AppServices';
import {useNavigationGuard} from '../../app/NavigationGuard';
import type {MusicTab} from '../../app/navigation';
import type {Playlist,PlaylistSummary} from '../../app/contracts';
import {errorMessage} from '../../app/backend';
import {PlaybackProvider,useOptionalPlayback,usePlayback} from '../player/PlaybackProvider';
import {musicApi} from './musicApi';
import {MusicLibrary} from './MusicLibrary';
import {MusicQueueView} from './MusicQueueView';
import {PlaylistList} from './PlaylistList';
import {PlaylistEditor} from './PlaylistEditor';
import {PageHeader} from '../../ui/PageHeader';
import {Dialog} from '../../ui/Dialog';
import {LibraryFolderActions} from '../library/LibraryFolderActions';
import {useMediaLibrary} from '../library/useMediaLibrary';
import './music.css';
interface Props {api?:typeof musicApi;tab?:MusicTab;onTabChange?:(tab:MusicTab)=>void}
export function MusicScreen(props:Props){const services=useAppServices(),shared=useOptionalPlayback();if(shared)return <MusicContent {...props}/>;return <AppServicesProvider services={{...services,music:props.api??services.music,canUseMedia:true}}><PlaybackProvider><MusicContent {...props}/></PlaybackProvider></AppServicesProvider>;}
function MusicContent({api:override,tab:controlled,onTabChange}:Props){
 const library=useMediaLibrary('audio');
 const services=useAppServices(),api=override??services.music,{queue,error:playbackError,refresh}=usePlayback(),register=useNavigationGuard();
 const [localTab,setTab]=useState<MusicTab>('library'),[playlists,setPlaylists]=useState<PlaylistSummary[]>([]),[draft,setDraft]=useState<Playlist|null>(null),[saved,setSaved]=useState(''),[pending,setPending]=useState(false),[error,setError]=useState(''),[notice,setNotice]=useState(''),[confirm,setConfirm]=useState(false);const proceed=useRef<()=>void>(()=>{}),lock=useRef(false);const tab=controlled??localTab,dirty=draft!==null&&JSON.stringify(draft)!==saved;
 useEffect(()=>{let active=true;void api.playlists().then(value=>{if(active)setPlaylists(value??[]);}).catch(e=>{if(active)setError(errorMessage(e));});return()=>{active=false;};},[api]);
 useEffect(()=>{register(dirty?(next)=>{proceed.current=next;setConfirm(true);}:null);return()=>register(null);},[register,dirty]);
 async function run(work:()=>Promise<void>){if(lock.current)return;lock.current=true;setPending(true);setError('');try{await work();}catch(e){setError(errorMessage(e));}finally{lock.current=false;setPending(false);}}
 function openDraft(value:Playlist){setDraft(value);setSaved(JSON.stringify(value));}
 function request(next:()=>void){if(dirty){proceed.current=next;setConfirm(true);}else next();}
 function changeTab(value:MusicTab){request(()=>{onTabChange?.(value);setTab(value);setDraft(null);});}
 async function save(andContinue=false){if(!draft||!draft.name.trim()||Array.from(draft.name.trim()).length>200||draft.items.length>10000)return;await run(async()=>{const id=await api.save(draft.id||null,draft.name.trim(),draft.items.map(i=>i.mediaId));const value={...draft,id,name:draft.name.trim()};setDraft(value);setSaved(JSON.stringify(value));setPlaylists(await api.playlists());setNotice('Playlist saved.');if(andContinue){setConfirm(false);setDraft(null);proceed.current();}});}
 return <><PageHeader title="Music"/><div className="music-tab-row"><div role="tablist" aria-label="Music views">{(['library','playlists','queue'] as const).map(value=><button key={value} role="tab" aria-selected={tab===value} aria-controls={`music-${value}`} id={`tab-${value}`} onClick={()=>changeTab(value)}>{value[0].toUpperCase()+value.slice(1)}</button>)}</div>{tab==='library'&&<LibraryFolderActions library={library}/>}</div>{((error&&!draft)||playbackError)&&<p role="alert" className="message">{error||playbackError}</p>}{notice&&<p role="status">{notice}</p>}<div role="tabpanel" id={`music-${tab}`} aria-labelledby={`tab-${tab}`}>{tab==='library'?<MusicLibrary library={library}/>:tab==='queue'?<MusicQueueView onSave={items=>openDraft({id:'',name:'',items})}/>:<PlaylistList playlists={playlists} pending={pending} onNew={()=>openDraft({id:'',name:'',items:[]})} onPlay={id=>void run(async()=>{await api.playPlaylist(id);await refresh();})} onEdit={id=>void run(async()=>openDraft(await api.playlist(id)))} onDelete={id=>void run(async()=>{await api.remove(id);setPlaylists(await api.playlists());setNotice('Playlist removed. Music files were kept.');})}/>}</div>{draft&&<PlaylistEditor error={error} draft={draft} dirty={dirty} pending={pending} onChange={setDraft} queueItems={queue?.items.map(e=>e.item)??[]} onSave={()=>void save()} onRequestClose={()=>request(()=>setDraft(null))}/>}<Dialog open={confirm} title="Save playlist changes?" busy={pending} onRequestClose={()=>setConfirm(false)}><p>Your playlist has unsaved changes.</p>{error&&<p role="alert">{error}</p>}<div className="actions"><button className="control primary" disabled={pending||!draft?.name.trim()||Array.from(draft?.name.trim()??'').length>200} onClick={()=>void save(true)}>Save</button><button className="control" disabled={pending} onClick={()=>{setConfirm(false);setDraft(null);proceed.current();}}>Discard</button><button className="control" disabled={pending} onClick={()=>setConfirm(false)}>Cancel</button></div></Dialog></>;
}
