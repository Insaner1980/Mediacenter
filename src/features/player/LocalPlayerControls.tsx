import {useRef,useState,type PointerEvent} from 'react';
import {usePlayback} from './PlaybackProvider';
import {formatTime} from './formatTime';
export function LocalPlayerControls(){
 const {player,error,pending:busy,playerAction:run}=usePlayback();const [seek,setSeek]=useState<{sessionId:string;position:number}|null>(null);
 const drag=useRef<{pointerId:number;sessionId:string}|null>(null);
 if(!player)return <section>{error?<p role="alert" className="message">{error}</p>:<p>No local player is open.</p>}</section>;
 const {positionSeconds:position,durationSeconds:duration,sessionId}=player;
 const draft=seek?.sessionId===sessionId?seek:null;
 function commitSeek(){
  if(draft)void run({type:'seek',seconds:draft.position},draft.sessionId);
  setSeek(null);
 }
 function beginDrag(event:PointerEvent<HTMLInputElement>){
  drag.current={pointerId:event.pointerId,sessionId:sessionId};
  setSeek(null);
 }
 function endDrag(event:PointerEvent<HTMLInputElement>,commit:boolean){
  const current=drag.current;
  if(!current||current.pointerId!==event.pointerId)return;
  drag.current=null;
  if(commit&&current.sessionId===sessionId)commitSeek();
  else setSeek(null);
 }
 function changeSeek(position:number){
  if(!drag.current||drag.current.sessionId===sessionId)setSeek({sessionId,position});
 }
 return <section className="full-player"><h1>Local playback</h1><p>{player.phase}</p><p className="mono">{formatTime(position)}{duration!==null?` / ${formatTime(duration)}`:''}</p><div className="actions"><button className="control" disabled={busy||['ended','error','stopped'].includes(player.phase)} onClick={()=>void run({type:'pause',paused:!player.paused})}>{player.paused?'Play':'Pause'}</button><button className="control" disabled={busy} onClick={()=>void run({type:'stop'})}>Stop</button><button className="control" disabled={busy} onClick={()=>void run({type:'mute',muted:!player.muted})}>{player.muted?'Unmute':'Mute'}</button></div><label className="field">Seek<input className="player-range" type="range" min="0" max={duration??0} step="1" disabled={busy||!player.seekable||duration===null} value={draft?.position??position} onChange={e=>changeSeek(Number(e.target.value))} onPointerDown={beginDrag} onPointerUp={e=>endDrag(e,true)} onPointerCancel={e=>endDrag(e,false)} onLostPointerCapture={e=>endDrag(e,false)} onKeyDown={()=>{drag.current=null;}} onKeyUp={()=>{if(!drag.current)commitSeek();}}/></label><label className="field">Volume<input className="player-range" type="range" min="0" max="100" step="1" disabled={busy} value={player.volume} onChange={e=>void run({type:'volume',value:Number(e.target.value)})}/></label><label className="field">Audio track<select disabled={busy} value={player.tracks.find(t=>t.kind==='audio'&&t.selected)?.id??''} onChange={e=>void run({type:'audio_track',id:Number(e.target.value)})}><option value="" disabled>No selected audio track</option>{player.tracks.filter(t=>t.kind==='audio').map(t=><option key={t.id} value={t.id}>{t.title} ({t.id})</option>)}</select></label><label className="field">Subtitles<select disabled={busy} value={player.tracks.find(t=>t.kind==='sub'&&t.selected)?.id??''} onChange={e=>void run({type:'subtitle_track',id:e.target.value?Number(e.target.value):null})}><option value="">Off</option>{player.tracks.filter(t=>t.kind==='sub').map(t=><option key={t.id} value={t.id}>{t.title} ({t.id})</option>)}</select></label>{(error||player.error) && <p role="alert" className="message">{error||player.error}</p>}</section>;
}
