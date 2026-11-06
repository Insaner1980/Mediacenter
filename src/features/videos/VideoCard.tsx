import type {MediaItem} from '../../app/contracts';
import {MediaThumbnail} from '../library/MediaThumbnail';
import {formatTime} from '../player/PlayerBar';
import {Icon} from '../../ui/Icons';
export function VideoCard({item,onPlay,disabled=false}:{item:MediaItem;onPlay:(resume:boolean)=>void;disabled?:boolean}){
 const available=item.availability==='available',resume=available&&(item.resumePositionMs??0)>=10000,duration=item.durationMs;
 const progress=duration!==null&&duration>0&&resume?Math.min(1,item.resumePositionMs!/duration):null;
 return <article className="video-card"><button className="video-card-play" aria-label={`Play ${item.displayName}`} disabled={disabled||!available} onClick={()=>onPlay(resume)}>
  <span className="video-image"><MediaThumbnail item={item}/><span className="video-play"><Icon name="play"/></span>{progress!==null&&<progress className="resume-progress" aria-label={`Progress ${item.displayName}`} value={progress} max={1}/>}</span>
  <span className="video-title" title={item.displayName}>{item.displayName}</span>
  <span className="video-meta">{duration!==null&&<span className="mono">{formatTime(duration/1000)}</span>}{resume&&<span className="resume-label">Continue at {formatTime(Math.min(item.resumePositionMs!,duration??Infinity)/1000)}</span>}{!available&&<span>{item.availability}</span>}</span>
 </button>{resume&&<button className="text-button restart-button" aria-label={`Play ${item.displayName} from start`} disabled={disabled} onClick={()=>onPlay(false)}>Play from start</button>}</article>;
}
