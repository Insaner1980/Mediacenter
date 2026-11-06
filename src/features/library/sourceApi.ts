import {isTauri} from '@tauri-apps/api/core';
import {listen} from '@tauri-apps/api/event';
import type {MediaSource} from '../../app/contracts';
export async function observeSources(update:(sources:MediaSource[])=>void):Promise<()=>void>{
 if(!isTauri())return()=>{};
 return listen<MediaSource[]>('sources_changed',({payload})=>update(payload));
}
