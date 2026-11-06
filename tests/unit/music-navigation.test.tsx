import {render,screen} from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import {expect,it,vi} from 'vitest';
import {App} from '../../src/app/App';
import {backend} from '../../src/app/backend';
vi.mock('@tauri-apps/api/core',()=>({isTauri:()=>true,invoke:vi.fn()}));
vi.mock('@tauri-apps/api/window',()=>({getCurrentWindow:()=>({label:'main'})}));
vi.mock('@tauri-apps/api/event',()=>({listen:async()=>()=>{}}));
vi.mock('../../src/app/backend',async importOriginal=>{
 const original=await importOriginal<typeof import('../../src/app/backend')>();
 const {referenceSnapshot}=await import('../../src/testing/fixtures');
 return {...original,subscribeHome:async(callback:(s:unknown)=>void)=>{callback({...referenceSnapshot,sessions:[{id:'current-music',kind:'local-audio',serviceId:null,mediaId:'song',phase:'paused',target:{monitorId:null,audioSinkId:'missing-hdmi'}}]});return()=>{};},backend:{...original.backend,countries:async()=>[],outputs:async()=>({monitors:[],audioSinks:[{id:'computer',name:'Computer speakers',available:true}],computerMonitorId:null,audioError:null}),moveSession:vi.fn(async()=>{})}};
});
it('moves and recovers currently playing music after navigating home to settings',async()=>{
 render(<App/>);
 await userEvent.click(await screen.findByRole('button',{name:'Settings'}));
 await userEvent.selectOptions(await screen.findByLabelText('Sound'),'computer');
 await userEvent.click(screen.getByText('Connection details'));await userEvent.click(screen.getByRole('button',{name:'Move playback'}));
 expect(backend.moveSession).toHaveBeenCalledWith('current-music',{monitorId:null,audioSinkId:'computer'});
 await userEvent.click(screen.getByRole('button',{name:'Bring back to computer'}));
 expect(backend.moveSession).toHaveBeenLastCalledWith('current-music',{monitorId:null,audioSinkId:null});
});
