import {render,screen,waitFor} from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import {beforeEach,expect,it,vi} from 'vitest';
import {App} from '../../src/app/App';
import {backend} from '../../src/app/backend';
import type {ManagedSession,ServiceId} from '../../src/app/contracts';

const state=vi.hoisted(()=>({sessions:[] as ManagedSession[]}));
vi.mock('@tauri-apps/api/core',()=>({isTauri:()=>true,invoke:vi.fn()}));
vi.mock('@tauri-apps/api/window',()=>({getCurrentWindow:()=>({label:'main'})}));
vi.mock('@tauri-apps/api/event',()=>({listen:async()=>()=>{}}));
vi.mock('../../src/app/backend',async importOriginal=>{
 const original=await importOriginal<typeof import('../../src/app/backend')>();
 const {referenceSnapshot}=await import('../../src/testing/fixtures');
 return {...original,subscribeHome:async(callback:(s:unknown)=>void)=>{callback({...referenceSnapshot,sessions:state.sessions});return()=>{};},backend:{...original.backend,openService:vi.fn(async()=>{}),focusSession:vi.fn(async()=>{}),outputs:async()=>({monitors:[],audioSinks:[],computerMonitorId:null,audioError:null})}};
});
beforeEach(()=>{state.sessions=[];vi.clearAllMocks();});
function hbo(phase:string):ManagedSession{return {id:'hbo-session',kind:'streaming',serviceId:'hbo_max',mediaId:null,phase,target:{monitorId:null,audioSinkId:null}};}

const services:[ServiceId,string][]=[['netflix','Netflix'],['youtube','YouTube'],['hbo_max','HBO Max'],['viaplay','Viaplay'],['mtv_katsomo','MTV Katsomo'],['ruutu','Ruutu'],['yle_areena','Yle Areena'],['plex','Plex']];
it.each(services.flatMap(([id,name])=>['none','ready','unavailable'].map(phase=>({id,name,phase}))))('opens $name directly from its tile with $phase previous session',async({id,name,phase})=>{
 if(phase!=='none')state.sessions=[{...hbo(phase),serviceId:id}];
 render(<App/>);
 await userEvent.click(await screen.findByRole('button',{name}));
 await waitFor(()=>expect(backend.openService).toHaveBeenCalledWith(id));
 expect(backend.focusSession).not.toHaveBeenCalled();
 expect(screen.getByRole('button',{name})).toBeVisible();
 expect(screen.queryByText(/Service window:/)).not.toBeInTheDocument();
});
it('opens service controls only when explicitly requested',async()=>{
 state.sessions=[hbo('ready')];render(<App/>);
 await userEvent.click(await screen.findByRole('button',{name:'Playback controls'}));
 expect(screen.getByText('Service window: ready')).toBeVisible();
 expect(backend.openService).not.toHaveBeenCalled();
});
it('uses the reopening operation from service controls too',async()=>{
 state.sessions=[hbo('unavailable')];render(<App/>);
 await userEvent.click(await screen.findByRole('button',{name:'Playback controls'}));
 await userEvent.click(screen.getByRole('button',{name:'Open service window'}));
 expect(backend.openService).toHaveBeenCalledWith('hbo_max');
});
it('keeps a launch failure beside the home tiles',async()=>{
 vi.mocked(backend.openService).mockRejectedValueOnce(new Error('Browser could not open'));
 render(<App/>);
 await userEvent.click(await screen.findByRole('button',{name:'HBO Max'}));
 expect(await screen.findByText('Browser could not open')).toBeVisible();
 expect(screen.getByRole('button',{name:'HBO Max'})).toBeVisible();
});
it('explains HBO output control without offering control of ordinary Chrome',async()=>{
 render(<App/>);
 await userEvent.click(await screen.findByRole('button',{name:'HBO Max'}));
 expect(await screen.findByText('Continue in Chrome for HBO Max. Choose its display and sound in your system settings.')).toBeVisible();
 expect(screen.queryByRole('button',{name:'Playback controls'})).not.toBeInTheDocument();
});
it.each(services.flatMap(([from])=>services.filter(([to])=>to!==from).map(([to,name])=>({from,to,name}))))('switches directly from $from to $to',async({from,to,name})=>{
 state.sessions=[{...hbo('ready'),serviceId:from}];render(<App/>);
 await userEvent.click(await screen.findByRole('button',{name}));
 await waitFor(()=>expect(backend.openService).toHaveBeenCalledWith(to));
 expect(screen.getByRole('button',{name})).toBeVisible();
 expect(screen.queryByText(/Service window:/)).not.toBeInTheDocument();
});
it('keeps a failed switch visible on home instead of showing the previous service screen',async()=>{
 state.sessions=[{...hbo('ready'),serviceId:'youtube'}];
 vi.mocked(backend.openService).mockRejectedValueOnce(new Error('Viaplay could not open'));
 render(<App/>);
 await userEvent.click(await screen.findByRole('button',{name:'Viaplay'}));
 expect(await screen.findByText('Viaplay could not open')).toBeVisible();
 expect(screen.getByRole('button',{name:'Viaplay'})).toBeVisible();
 expect(screen.queryByText(/Service window:/)).not.toBeInTheDocument();
});
