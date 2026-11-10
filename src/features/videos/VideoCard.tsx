import type {MediaItem} from '../../app/contracts';
import {MediaThumbnail} from '../library/MediaThumbnail';
import {formatTime} from '../player/formatTime';
import {Icon} from '../../ui/Icons';
export function VideoCard({item,onPlay,disabled=false}:{item:MediaItem;onPlay:(resume:boolean)=>void;disabled?:boolean}){
 const available=item.availability==='available',resume=available&&(item.resumePositionMs??0)>=10000,duration=item.durationMs;
 const progress=duration!==null&&duration>0&&resume?Math.min(1,item.resumePositionMs!/duration):null;
 return <article className="video-card"><button data-remote-id={`video-${item.id}`} data-remote-group="content" data-remote-media={item.id} data-remote-kind="video" data-remote-resume={resume} className="video-card-play" aria-label={`Play ${item.displayName}`} disabled={disabled||!available} onClick={()=>onPlay(resume)}>
  <span className="video-image"><MediaThumbnail item={item}/><span className="video-play"><Icon name="play"/></span>{progress!==null&&<progress className="resume-progress" aria-label={`Progress ${item.displayName}`} value={progress} max={1}/>}</span>
  <span className="video-title" title={item.displayName}>{item.displayName}</span>
  <span className="video-meta">{duration!==null&&<span className="mono">{formatTime(duration/1000)}</span>}{resume&&<span className="resume-label">Continue at {formatTime(Math.min(item.resumePositionMs!,duration??Infinity)/1000)}</span>}{!available&&<span>{item.availability}</span>}</span>
 </button>{resume&&<button data-remote-id={`restart-${item.id}`} data-remote-group="content" data-remote-media={item.id} data-remote-kind="video" data-remote-resume="false" className="text-button restart-button" aria-label={`Play ${item.displayName} from start`} disabled={disabled} onClick={()=>onPlay(false)}>Play from start</button>}</article>;
}
