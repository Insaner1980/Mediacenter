import {listen} from '@tauri-apps/api/event';
import {backend} from '../../app/backend';
import type {PlayerSnapshot} from '../../app/contracts';
export const playerApi={
 async observe(update:(value:PlayerSnapshot|null)=>void):Promise<()=>void>{
  let active=true, generation=0;let current:PlayerSnapshot|null=null;
  const accept=(value:PlayerSnapshot|null)=>{if(!active)return;if(value&&current&&value.sessionId===current.sessionId&&value.revision<current.revision)return;current=value;update(value);};
  const stop=await listen<PlayerSnapshot|null>('player_changed',({payload})=>{generation++;accept(payload);});
  const before=generation;
  try{const value=await backend.player();if(generation===before)accept(value);}catch(error){active=false;stop();throw error;}
  return()=>{active=false;stop();};
 }
};
