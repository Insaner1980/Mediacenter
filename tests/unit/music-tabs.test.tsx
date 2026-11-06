import {render,screen,waitFor} from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import {expect,it,vi} from 'vitest';
import {MusicScreen} from '../../src/features/music/MusicScreen';
import {AppServicesProvider,productionServices} from '../../src/app/AppServices';
const queue={revision:7,sessionId:null,items:[],originalMediaIds:[],currentIndex:null,shuffle:false,repeat:'off' as const,message:null};
it('starts in Library and confirms dirty playlist closure with Cancel and Discard',async()=>{
 const services={...productionServices,canUseMedia:true,backend:{...productionServices.backend,player:async()=>null,sources:async()=>[],queryMedia:async()=>({items:[],nextCursor:null})},player:{observe:async()=>()=>{}},music:{...productionServices.music,queue:async()=>queue,subscribe:async()=>()=>{},playlists:async()=>[],save:vi.fn(async()=> 'new')}};
 const user=userEvent.setup();render(<AppServicesProvider services={services}><MusicScreen/></AppServicesProvider>);
 expect(screen.getByRole('tab',{name:'Library'})).toHaveAttribute('aria-selected','true');await user.click(screen.getByRole('tab',{name:'Playlists'}));await user.click(screen.getByRole('button',{name:'New playlist'}));await user.type(screen.getByLabelText('Playlist name'),'Draft');expect(services.music.save).not.toHaveBeenCalled();await user.click(screen.getByRole('button',{name:'Close'}));await user.click(screen.getByRole('button',{name:'Cancel'}));expect(screen.getByLabelText('Playlist name')).toHaveValue('Draft');await user.click(screen.getByRole('button',{name:'Close'}));await user.click(screen.getByRole('button',{name:'Discard'}));expect(screen.queryByLabelText('Playlist name')).not.toBeInTheDocument();
});
it('shows an explicit save failure inside the open editor',async()=>{
 const services={...productionServices,canUseMedia:true,backend:{...productionServices.backend,player:async()=>null,sources:async()=>[],queryMedia:async()=>({items:[],nextCursor:null})},player:{observe:async()=>()=>{}},music:{...productionServices.music,queue:async()=>queue,subscribe:async()=>()=>{},playlists:async()=>[],save:vi.fn(async()=>{throw Error('Playlist could not be saved');})}};
 const user=userEvent.setup();render(<AppServicesProvider services={services}><MusicScreen/></AppServicesProvider>);await user.click(screen.getByRole('tab',{name:'Playlists'}));await user.click(screen.getByText('New playlist'));await user.type(screen.getByLabelText('Playlist name'),'Draft');await user.click(screen.getByText('Save playlist'));const alert=await screen.findByRole('alert');expect(screen.getByRole('dialog')).toContainElement(alert);expect(screen.getByLabelText('Playlist name')).toHaveValue('Draft');
});
it('only offers folder opening in the Library tab where its feedback is visible',async()=>{
 const services={...productionServices,canUseMedia:true,backend:{...productionServices.backend,player:async()=>null,sources:async()=>[],queryMedia:async()=>({items:[],nextCursor:null})},player:{observe:async()=>()=>{}},music:{...productionServices.music,queue:async()=>queue,subscribe:async()=>()=>{},playlists:async()=>[]}};
 const user=userEvent.setup();render(<AppServicesProvider services={services}><MusicScreen/></AppServicesProvider>);expect(screen.getByRole('button',{name:'Open folder'})).toBeVisible();await user.click(screen.getByRole('tab',{name:'Queue'}));expect(screen.queryByRole('button',{name:'Open folder'})).not.toBeInTheDocument();await user.click(screen.getByRole('tab',{name:'Playlists'}));expect(screen.queryByRole('button',{name:'Open folder'})).not.toBeInTheDocument();
});

it('plays from the track title and row cells, while enqueue only adds to the queue',async()=>{
 const play=vi.fn(async()=>{}),control=vi.fn(async()=>queue);
 const services={...productionServices,canUseMedia:true,backend:{...productionServices.backend,player:async()=>null,sources:async()=>[],queryMedia:async()=>({items:[{id:'song',displayName:'Test song',kind:'audio' as const,sourceId:'folder',availability:'available',durationMs:60000,resumePositionMs:null,thumbnailId:null}],nextCursor:null})},player:{observe:async()=>()=>{}},music:{...productionServices.music,queue:async()=>queue,subscribe:async()=>()=>{},playlists:async()=>[],play,control}};
 const user=userEvent.setup();render(<AppServicesProvider services={services}><MusicScreen/></AppServicesProvider>);
 await user.click(await screen.findByText('Test song'));await waitFor(()=>expect(play).toHaveBeenCalledWith(['song']));
 await user.click(screen.getByText('1:00'));await waitFor(()=>expect(play).toHaveBeenCalledTimes(2));
 await user.click(screen.getByRole('button',{name:'Add Test song to queue'}));expect(control).toHaveBeenCalled();expect(play).toHaveBeenCalledTimes(2);
});
