import {invoke} from '@tauri-apps/api/core';
import {listen} from '@tauri-apps/api/event';
import {performOperation} from '../../app/backend';
import type {MusicAction,MusicQueue,Playlist,PlaylistSummary,PlayerSnapshot} from '../../app/contracts';
export const musicApi={
 queue:()=>invoke<MusicQueue>('get_music_queue'),
 control:(queue:MusicQueue,action:MusicAction)=>invoke<MusicQueue>('control_music',{sessionId:queue.sessionId,revision:queue.revision,action}),
 play:(mediaIds:string[],startIndex=0)=>performOperation('play_music_queue',{mediaIds,startIndex}),
 playlists:()=>invoke<PlaylistSummary[]>('list_playlists'),
 playlist:(playlistId:string)=>invoke<Playlist>('get_playlist',{playlistId}),
 save:(playlistId:string|null,name:string,mediaIds:string[])=>invoke<string>('save_playlist',{playlistId,name,mediaIds}),
 remove:(playlistId:string)=>invoke<void>('remove_playlist',{playlistId}),
 playPlaylist:(playlistId:string,startIndex=0)=>performOperation('play_playlist',{playlistId,startIndex}),
 subscribe:async(update:(queue:MusicQueue)=>void)=>{
  const unlistenQueue=await listen<MusicQueue>('music_queue_changed',({payload})=>update(payload));
  try{
   const unlistenPlayer=await listen<PlayerSnapshot|null>('player_changed',({payload})=>{if(payload===null)void invoke<MusicQueue>('get_music_queue').then(update).catch(()=>{});});
   return()=>{unlistenQueue();unlistenPlayer();};
  }catch(error){unlistenQueue();throw error;}
 },
};
