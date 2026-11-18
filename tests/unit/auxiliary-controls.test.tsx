import {render,screen,waitFor} from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import {expect,it,vi} from 'vitest';
import {AppServicesProvider,productionServices} from '../../src/app/AppServices';
import {PlaybackProvider} from '../../src/features/player/PlaybackProvider';
import {LocalPlayerControls} from '../../src/features/player/LocalPlayerControls';
import type {PlayerSnapshot} from '../../src/app/contracts';
it('keeps mute, stop, audio selection and subtitle off reachable',async()=>{
 const player:PlayerSnapshot={revision:1,sessionId:'session',mediaId:'video',phase:'playing',positionSeconds:0,durationSeconds:60,paused:false,volume:80,muted:false,seekable:true,error:null,tracks:[{id:1,kind:'audio',title:'English',selected:true},{id:2,kind:'audio',title:'Finnish',selected:false},{id:3,kind:'sub',title:'Finnish',selected:true}]};const command=vi.fn(async()=>{});const services={...productionServices,canUseMedia:true,backend:{...productionServices.backend,player:async()=>player,controlPlayer:command},music:{...productionServices.music,subscribe:async()=>()=>{},queue:async()=>({revision:1,sessionId:null,items:[],originalMediaIds:[],currentIndex:null,shuffle:false,repeat:'off' as const,message:null})},player:{observe:async(update:(p:PlayerSnapshot|null)=>void)=>{update(player);return()=>{};}}};
 const user=userEvent.setup();render(<AppServicesProvider services={services}><PlaybackProvider><LocalPlayerControls/></PlaybackProvider></AppServicesProvider>);await user.selectOptions(await screen.findByLabelText('Audio track'),'2');await waitFor(()=>expect(command).toHaveBeenLastCalledWith('session',{type:'audio_track',id:2}));await user.selectOptions(screen.getByLabelText('Subtitles'),'');expect(command).toHaveBeenLastCalledWith('session',{type:'subtitle_track',id:null});await user.click(screen.getByRole('button',{name:'Mute'}));expect(command).toHaveBeenLastCalledWith('session',{type:'mute',muted:true});await user.click(screen.getByRole('button',{name:'Stop'}));expect(command).toHaveBeenLastCalledWith('session',{type:'stop'});
});
