import {invoke} from '@tauri-apps/api/core';
import {listen} from '@tauri-apps/api/event';
import type {PlayerSnapshot,PlayerAction,MediaPage,EncodedImage,SourceGrant,MediaSource,MediaKind,DriveDescriptor,OutputList,PlaybackTarget,HomeSnapshot,Theme,CountryOption,VpnSnapshot,ServiceId,OperationRef,OperationFinished,AppError} from './contracts';
import {RevisionStore} from './controller';
export const backend = {
  player:()=>invoke<PlayerSnapshot|null>('get_player'),
  playMedia:(mediaId:string,resume:boolean)=>performOperation('play_media',{mediaId,resume}),
  controlPlayer:(sessionId:string,action:PlayerAction)=>invoke<void>('control_player',{sessionId,action}),
  queryMedia:(kind:MediaKind,sourceId:string|null,query:string,cursor:string|null)=>invoke<MediaPage>('query_media',{kind,sourceId,query,cursor,limit:50}),
  thumbnail:(mediaId:string)=>invoke<EncodedImage>('get_thumbnail',{mediaId}),
  scanSource:(sourceId:string)=>performOperation('rescan_source',{sourceId},0),
  cancelScan:(sourceId:string)=>invoke<void>('cancel_source_scan',{sourceId}),
  chooseSource:()=>invoke<SourceGrant|null>('choose_media_source'),
  sources:()=>invoke<MediaSource[]>('list_media_sources'),
  addSource:(grantId:string,kinds:MediaKind[])=>invoke<string>('add_media_source',{grantId,kinds}),
  removeSource:(sourceId:string)=>invoke<void>('remove_media_source',{sourceId}),
  drives:()=>invoke<DriveDescriptor[]>('list_drives'),
  setControlMonitor:(monitorId:string)=>invoke<void>('set_control_monitor',{monitorId}),
  outputs:()=>invoke<OutputList>('list_outputs'),
  selectOutput:(target:PlaybackTarget)=>invoke<void>('select_output',{target}),
  moveSession:(sessionId:string,target:PlaybackTarget)=>performOperation('move_session',{sessionId,target}),
  snapshot:()=>invoke<HomeSnapshot>('get_home_snapshot'),
  setTheme:(theme:Theme)=>invoke<Theme>('set_theme',{theme}),
  refreshVpn:()=>invoke<VpnSnapshot>('refresh_vpn'),
  countries:()=>invoke<CountryOption[]>('list_vpn_countries'),
  connect:(country:string|null)=>performOperation('connect_vpn',{country}),
  disconnect:()=>performOperation('disconnect_vpn'),
  enableKillSwitch:()=>performOperation('enable_kill_switch'),
  setVpnRequirement:(serviceId:ServiceId,required:boolean)=>invoke<boolean>('set_vpn_requirement',{serviceId,required}),
  openService:(serviceId:ServiceId)=>performOperation('open_service',{serviceId}),
  focusSession:(sessionId:string)=>invoke<void>('focus_session',{sessionId}),
  closeSession:(sessionId:string)=>performOperation('close_session',{sessionId}),
};
export function errorMessage(error:unknown):string {
  if(typeof error==='object' && error && 'message' in error && typeof error.message==='string')return error.message;
  return 'The operation could not be completed. Try again.';
}
export async function performOperation(command:string,args:Record<string,unknown>={},timeoutMs=75000):Promise<void> {
  let id:string|null=null;let finish:((event:OperationFinished)=>void)|undefined;
  const early:OperationFinished[]=[];
  const unlisten=await listen<OperationFinished>('operation_finished',({payload})=>{if(id===payload.operationId)finish?.(payload);else if(!id && early.length<64)early.push(payload);});
  let timer:ReturnType<typeof setTimeout>|undefined;
  try {
    const terminal=new Promise<void>((resolve,reject)=>{finish=event=>event.error?reject(event.error):resolve();});
    const operation=await invoke<OperationRef>(command,args);id=operation.operationId;
    const received=early.find(event=>event.operationId===id);if(received)finish?.(received);
    const timeout=new Promise<void>((_,reject)=>{if(timeoutMs>0)timer=setTimeout(()=>{void invoke('cancel_operation',{operationId:id}).catch(()=>{});reject({message:'The operation timed out. Its latest state will be refreshed.'} satisfies Partial<AppError>);},timeoutMs);});
    await Promise.race([terminal,timeout]);
  } finally {clearTimeout(timer);unlisten();}
}
export async function subscribeHome(update:(value:HomeSnapshot)=>void):Promise<()=>void> {
  const store=new RevisionStore<HomeSnapshot>();let active=true;let starting=true;const pending:HomeSnapshot[]=[];
  let refreshing:Promise<void>|null=null;
  async function refresh() {
    if(refreshing)return refreshing;
    refreshing=backend.snapshot().then(value=>{if(active){store.reset(value);update(store.value!);}}).finally(()=>{refreshing=null;});
    return refreshing;
  }
  const unlisten=await listen<HomeSnapshot>('home_changed',({payload})=>{
    if(!active)return;
    if(starting){pending.push(payload);return;}
    const result=store.accept(payload);
    if(result==='gap')void refresh().catch(()=>{});
    if(result==='applied')update(payload);
  });
  try {
    await refresh();starting=false;
    for(const value of pending.sort((a,b)=>a.revision-b.revision)){
      const result=store.accept(value);if(result==='applied' && active)update(value);else if(result==='gap')await refresh();
    }
  } catch(error) {active=false;unlisten();throw error;}
  return ()=>{active=false;unlisten();};
}
