import {MainApp} from '../../src/app/App';
import {AppServicesProvider} from '../../src/app/AppServices';
import {createFixtureServices} from '../../src/testing/fixtureServices';
import {useState} from 'react';
import {act,render,screen,waitFor} from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import {beforeEach,expect,it,vi} from 'vitest';
import {RemoteNavigationBridge} from '../../src/features/remote/RemoteNavigationBridge';
import {Sidebar} from '../../src/ui/Sidebar';
import type {AppRoute} from '../../src/app/navigation';
import type {ConfirmationPreview,UiTopic} from '../../src/features/remote/protocol.generated';
import type {RendererRequest,RendererResponse} from '../../src/features/remote/remoteNavigation';
const transport=vi.hoisted(()=>({listeners:new Map<string,(e:{payload:unknown})=>void>(),invoke:vi.fn()}));
vi.mock('@tauri-apps/api/core',()=>({isTauri:()=>true,invoke:transport.invoke}));
vi.mock('@tauri-apps/api/event',()=>({listen:async(name:string,callback:(e:{payload:unknown})=>void)=>{transport.listeners.set(name,callback);return()=>transport.listeners.delete(name);}}));
let previews:ConfirmationPreview[],context:UiTopic|undefined,responses:RendererResponse[];
const preview:ConfirmationPreview={confirmationId:'saved-id',ownerDeviceId:'phone-b',action:'change_audio',affectedLabels:['HDMI'],expected:{serverEpoch:'epoch'},expiresInMs:60000,confirmMethod:'audio.confirm'};
beforeEach(()=>{previews=[];context=undefined;responses=[];transport.listeners.clear();transport.invoke.mockReset();transport.invoke.mockImplementation(async(command:string,args?:Record<string,unknown>)=>{if(command==='register_remote_renderer')return '7';if(command==='get_renderer_confirmations')return previews;if(command==='update_remote_renderer')context=args?.context as UiTopic;if(command==='acknowledge_remote_renderer')responses.push(args?.response as RendererResponse);if(command==='resolve_renderer_confirmation')previews=[];});});
function Example(){const [route,setRoute]=useState<AppRoute>('home');return <><RemoteNavigationBridge route={route} navigate={setRoute}/><Sidebar route={route} onNavigate={setRoute}/><h1>{route}</h1></>;}
async function send(method:'ui.back'|'ui.activate'|'ui.navigate',params:Record<string,unknown>={}){const event={dispatchId:'unique-native-id',requestId:'wire-id',origin:'connection-a',ownerDeviceId:'phone-a',expiresAtMs:Date.now()+4000,request:{method,params:{rendererGeneration:'7',focusRevision:context!.focusRevision,modalId:context!.modalId,...params}}} as RendererRequest;await act(async()=>{transport.listeners.get('remote-ui-request')?.({payload:event});});await waitFor(()=>expect(responses).toHaveLength(1));return responses[0];}
it('acknowledges only after actual React navigation commits',async()=>{render(<Example/>);await waitFor(()=>expect(context?.available).toBe(true));const response=await send('ui.navigate',{route:'videos'});expect(screen.getByRole('heading',{name:'video'})).toBeVisible();expect(response.context.route).toBe('videos');expect(response.error).toBeUndefined();});
it('shares the server ID and closes desktop presentation after native resolution',async()=>{previews=[preview];render(<Example/>);await screen.findByRole('dialog');expect(screen.getByRole('button',{name:'Confirm'})).toHaveAttribute('data-remote-confirm','saved-id');await userEvent.click(screen.getByRole('button',{name:'Confirm'}));await waitFor(()=>expect(screen.queryByRole('dialog')).not.toBeInTheDocument());expect(transport.invoke).toHaveBeenCalledWith('resolve_renderer_confirmation',{confirmationId:'saved-id',confirm:true});});
it('loads pending confirmations even if the initial context update fails',async()=>{
 const normal=transport.invoke.getMockImplementation()!;
 let firstUpdate=true;
 transport.invoke.mockImplementation(async(command:string,args?:Record<string,unknown>)=>{
  if(command==='update_remote_renderer'&&firstUpdate){firstUpdate=false;throw new Error('Temporary publish failure');}
  return normal(command,args);
 });
 previews=[preview];render(<Example/>);
 expect(await screen.findByRole('dialog')).toBeVisible();
 await waitFor(()=>expect(context?.modalId).toBe('saved-id'));
});
it('keeps the replacement renderer current after an older registration completes',async()=>{
 const normal=transport.invoke.getMockImplementation()!;
 let requests=0,generation='0',release!:()=>void;
 const slowRegistration=new Promise<void>(resolve=>{release=resolve;});
 transport.invoke.mockImplementation(async(command:string,args?:Record<string,unknown>)=>{
  if(command==='register_remote_renderer'){
   const id=String(++requests);
   if(id==='1')await slowRegistration;
   generation=id;
   return id;
  }
  if(command==='update_remote_renderer'&&args?.generation!==generation)throw new Error('Stale renderer');
  return normal(command,args);
 });
 const first=render(<Example/>);
 await waitFor(()=>expect(requests).toBe(1));
 first.unmount();
 render(<Example/>);
 await act(async()=>{});
 await act(async()=>release());
 await userEvent.click(screen.getByRole('button',{name:'Videos'}));
 await waitFor(()=>expect(context?.route).toBe('videos'));
});
it('phone resolution removes the same desktop sheet',async()=>{previews=[preview];render(<Example/>);await screen.findByRole('dialog');previews=[];await act(async()=>transport.listeners.get('remote-ui-confirmation')?.({payload:{dispatchId:'presentation-id',requestId:'presentation-id',rendererGeneration:'7',presentationRevision:'1',expiresAtMs:Date.now()+4000,previews:[]}}));await waitFor(()=>expect(screen.queryByRole('dialog')).not.toBeInTheDocument());});
it('remote Back and Activate carry a typed confirmation ID without invoking local owner authority',async()=>{previews=[preview];render(<Example/>);await screen.findByRole('dialog');await waitFor(()=>expect(context?.modalId).toBeTruthy());const response=await send('ui.back');expect(response.action).toEqual({kind:'confirmation',confirmationId:'saved-id',confirm:false});expect(transport.invoke.mock.calls.some(([name])=>name==='resolve_renderer_confirmation')).toBe(false);expect(screen.getByRole('dialog')).toBeVisible();});
it('remote confirmation activation never calls the local click handler',async()=>{previews=[preview];render(<Example/>);await screen.findByRole('dialog');act(()=>screen.getByRole('button',{name:'Confirm'}).focus());await waitFor(()=>expect(context?.focusId).toBe('confirm-saved-id'));const response=await send('ui.activate',{focusId:'confirm-saved-id'});expect(response.action).toEqual({kind:'confirmation',confirmationId:'saved-id',confirm:true});expect(transport.invoke.mock.calls.some(([name])=>name==='resolve_renderer_confirmation')).toBe(false);});

it('remote page navigation cannot initialize or scan a local default media source',async()=>{
 const services=createFixtureServices({screen:'home',state:'populated',theme:'dark'});const initialize=vi.spyOn(services.backend,'ensureDefaultSource');render(<AppServicesProvider services={services}><MainApp/></AppServicesProvider>);await waitFor(()=>expect(context?.available).toBe(true));await send('ui.navigate',{route:'videos'});expect(screen.getByRole('heading',{name:'Videos'})).toBeVisible();expect(initialize).not.toHaveBeenCalled();
});

it('Back uses route history and preserves the existing dirty-playlist guard',async()=>{
 const services=createFixtureServices({screen:'home',state:'populated',theme:'dark'});render(<AppServicesProvider services={services}><MainApp/></AppServicesProvider>);await waitFor(()=>expect(context?.available).toBe(true));
 await send('ui.navigate',{route:'videos'});responses=[];await send('ui.navigate',{route:'photos'});responses=[];await send('ui.back');expect(screen.getByRole('heading',{name:'Videos'})).toBeVisible();
 responses=[];await send('ui.navigate',{route:'music'});await userEvent.click(screen.getByRole('tab',{name:'Playlists'}));await userEvent.click(screen.getByRole('button',{name:'New playlist'}));await userEvent.type(screen.getByLabelText('Playlist name'),'Draft');await waitFor(()=>expect(context?.modalId).toBeTruthy());
 responses=[];await send('ui.back');expect(screen.getByRole('dialog',{name:'Save playlist changes?'})).toBeVisible();expect(screen.getByLabelText('Playlist name')).toHaveValue('Draft');
});

it('applies typed confirmation presentation before ack and rejects old generations and snapshots',async()=>{
 render(<Example/>);await waitFor(()=>expect(context?.available).toBe(true));
 const present=async(revision:string,generation:string,values:ConfirmationPreview[])=>{await act(async()=>{transport.listeners.get('remote-ui-confirmation')?.({payload:{dispatchId:`present-${revision}-${generation}`,requestId:'presentation',rendererGeneration:generation,presentationRevision:revision,expiresAtMs:Date.now()+4000,previews:values}});});};
 await present('1','6',[preview]);expect(screen.queryByRole('dialog')).not.toBeInTheDocument();expect(responses.at(-1)?.error).toBe('stale_state');
 await present('2','7',[preview]);expect(screen.getByRole('dialog')).toBeVisible();expect(responses.at(-1)?.context.modalId).toBe('saved-id');
 await present('3','7',[]);expect(screen.queryByRole('dialog')).not.toBeInTheDocument();expect(responses.at(-1)?.context.modalId).toBeUndefined();
 await present('2','7',[preview]);expect(screen.queryByRole('dialog')).not.toBeInTheDocument();expect(responses.at(-1)?.error).toBe('stale_state');
});
it('remote sidebar activation also cannot initialize a local default folder',async()=>{
 const services=createFixtureServices({screen:'home',state:'populated',theme:'dark'});const initialize=vi.spyOn(services.backend,'ensureDefaultSource');render(<AppServicesProvider services={services}><MainApp/></AppServicesProvider>);await waitFor(()=>expect(context?.available).toBe(true));await send('ui.navigate',{route:'videos'});responses=[];act(()=>screen.getByRole('button',{name:'Music'}).focus());await waitFor(()=>expect(context?.focusId).toBe('sidebar-audio'));await send('ui.activate',{focusId:'sidebar-audio'});expect(screen.getByRole('heading',{name:'Music'})).toBeVisible();expect(initialize).not.toHaveBeenCalled();
});
