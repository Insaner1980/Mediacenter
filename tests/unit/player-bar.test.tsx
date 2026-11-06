import {act,fireEvent,render,screen,waitFor} from '@testing-library/react';
import {expect,it,vi} from 'vitest';
import {AppServicesProvider,productionServices} from '../../src/app/AppServices';
import {PlaybackProvider} from '../../src/features/player/PlaybackProvider';
import {PlayerBar} from '../../src/features/player/PlayerBar';
import type {MusicQueue,PlayerSnapshot} from '../../src/app/contracts';
const player:PlayerSnapshot={revision:7,sessionId:'audio-session',mediaId:'song',phase:'playing',positionSeconds:12,durationSeconds:null,paused:false,volume:50,muted:false,seekable:true,tracks:[],error:null};
const queue:MusicQueue={revision:7,sessionId:'audio-session',items:[{entryId:1,item:{mediaId:'song',title:'Song',artist:null,album:null,availability:'available'}}],originalMediaIds:['song'],currentIndex:0,shuffle:false,repeat:'off',message:null};
it('disables unknown-duration seek and delivers the last volume even during a pending command',async()=>{
 let finish:()=>void=()=>{};const command=vi.fn().mockImplementationOnce(()=>new Promise<void>(r=>{finish=r;})).mockResolvedValue(undefined);
 const fake={...productionServices,canUseMedia:true,backend:{...productionServices.backend,controlPlayer:command,player:async()=>player},player:{observe:async(update:(p:PlayerSnapshot|null)=>void)=>{update(player);return()=>{};}},music:{...productionServices.music,subscribe:async()=>()=>{},queue:async()=>queue}};
 render(<AppServicesProvider services={fake}><PlaybackProvider><PlayerBar onOpenQueue={()=>{}} onOpenPlayer={()=>{}}/></PlaybackProvider></AppServicesProvider>);
 expect(await screen.findByRole('slider',{name:'Seek'})).toBeDisabled();
 fireEvent.change(screen.getByRole('slider',{name:'Volume'}),{target:{value:'20'}});await waitFor(()=>expect(command).toHaveBeenCalled());
 fireEvent.change(screen.getByRole('slider',{name:'Volume'}),{target:{value:'37'}});await act(async()=>finish());
 await waitFor(()=>expect(command).toHaveBeenLastCalledWith('audio-session',{type:'volume',value:37}));expect(screen.getAllByRole('region',{name:'Playback'})).toHaveLength(1);
});
it('flushes a volume chosen during the reconciliation fetch',async()=>{
 let resolve:(v:PlayerSnapshot)=>void=()=>{};const command=vi.fn(async()=>{}),read=vi.fn().mockImplementationOnce(()=>new Promise<PlayerSnapshot>(r=>{resolve=r;})).mockResolvedValue(player);
 const fake={...productionServices,canUseMedia:true,backend:{...productionServices.backend,controlPlayer:command,player:read},player:{observe:async(update:(p:PlayerSnapshot|null)=>void)=>{update(player);return()=>{};}},music:{...productionServices.music,subscribe:async()=>()=>{},queue:async()=>queue}};
 render(<AppServicesProvider services={fake}><PlaybackProvider><PlayerBar onOpenQueue={()=>{}} onOpenPlayer={()=>{}}/></PlaybackProvider></AppServicesProvider>);
 fireEvent.change(await screen.findByRole('slider',{name:'Volume'}),{target:{value:'20'}});await waitFor(()=>expect(read).toHaveBeenCalled());fireEvent.change(screen.getByRole('slider',{name:'Volume'}),{target:{value:'37'}});await act(async()=>resolve(player));await waitFor(()=>expect(command).toHaveBeenLastCalledWith('audio-session',{type:'volume',value:37}));
});

it('does not display video transport or a stale video error while browsing',async()=>{
 const fake={...productionServices,canUseMedia:true,player:{observe:async(update:(p:PlayerSnapshot|null)=>void)=>{update({...player,sessionId:'video-session',error:'The player connection closed.'});return()=>{};}},music:{...productionServices.music,subscribe:async()=>()=>{},queue:async()=>queue}};
 await act(async()=>{render(<AppServicesProvider services={fake}><PlaybackProvider><PlayerBar onOpenQueue={()=>{}} onOpenPlayer={()=>{}}/></PlaybackProvider></AppServicesProvider>);});
 expect(screen.queryByRole('region',{name:'Playback'})).not.toBeInTheDocument();
});
