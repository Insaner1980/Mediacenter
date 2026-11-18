import {act,fireEvent,render,screen,waitFor} from '@testing-library/react';
import {expect,it,vi} from 'vitest';
import {AppServicesProvider,productionServices} from '../../src/app/AppServices';
import {PlaybackProvider,usePlayback} from '../../src/features/player/PlaybackProvider';
import {LocalPlayerControls} from '../../src/features/player/LocalPlayerControls';
import type {MusicQueue,PlayerSnapshot} from '../../src/app/contracts';
export const player:PlayerSnapshot={revision:7,sessionId:'audio-session',mediaId:'song',phase:'playing',positionSeconds:12,durationSeconds:60,paused:false,volume:50,muted:false,seekable:true,tracks:[],error:null};
export const queue:MusicQueue={revision:7,sessionId:'audio-session',items:[{entryId:1,item:{mediaId:'song',title:'Song',artist:null,album:null,availability:'available'}}],originalMediaIds:['song'],currentIndex:0,shuffle:false,repeat:'off',message:null};
function Observer(){const state=usePlayback();return <><p>Queue {state.queue?.revision}</p><p>Player {state.player?.revision}</p>{state.queueError&&<p role="alert">{state.queueError}</p>}<button onClick={()=>void state.musicAction({type:'next'})}>Next</button><button onClick={()=>void state.refreshQueue()}>Refresh queue</button></>;}
it('keeps newer queue observations over delayed fetch and mutation responses',async()=>{
 let emit:(q:MusicQueue)=>void=()=>{};let resolve:(q:MusicQueue)=>void=()=>{};
 const fake={...productionServices,canUseMedia:true,player:{observe:async(update:(p:PlayerSnapshot|null)=>void)=>{update(player);return()=>{};}},music:{...productionServices.music,subscribe:async(update:(q:MusicQueue)=>void)=>{emit=update;return()=>{};},queue:()=>new Promise<MusicQueue>(r=>{resolve=r;}),control:vi.fn(async()=>({...queue,revision:6}))}};
 render(<AppServicesProvider services={fake}><PlaybackProvider><Observer/></PlaybackProvider></AppServicesProvider>);
 await act(async()=>{});await act(async()=>emit({...queue,revision:8}));await act(async()=>resolve(queue));
 expect(screen.getByText('Queue 8')).toBeVisible();screen.getByText('Next').click();await waitFor(()=>expect(fake.music.control).toHaveBeenCalledWith(expect.objectContaining({revision:8}),{type:'next'}));expect(screen.getByText('Queue 8')).toBeVisible();
});
it('disposes subscriptions that resolve after unmount',async()=>{
 let resolve:(stop:()=>void)=>void=()=>{};const stop=vi.fn();
 const fake={...productionServices,canUseMedia:true,player:{observe:()=>new Promise<()=>void>(r=>{resolve=r;})},music:{...productionServices.music,subscribe:async()=>()=>{},queue:async()=>queue}};
 const view=render(<AppServicesProvider services={fake}><PlaybackProvider><Observer/></PlaybackProvider></AppServicesProvider>);view.unmount();await act(async()=>resolve(stop));expect(stop).toHaveBeenCalledTimes(1);
});
it('retries failed player observation and disposes the recovered subscription',async()=>{
 const stop=vi.fn();
 const observe=vi.fn().mockRejectedValueOnce(Error('Player unavailable')).mockImplementation(async(update:(value:PlayerSnapshot|null)=>void)=>{update(player);return stop;});
 const services={...productionServices,canUseMedia:true,player:{observe},music:{...productionServices.music,subscribe:async()=>()=>{},queue:async()=>queue}};
 const view=render(<AppServicesProvider services={services}><PlaybackProvider><Observer/></PlaybackProvider></AppServicesProvider>);
 expect(await screen.findByText('Player 7')).toBeVisible();
 view.unmount();
 expect(stop).toHaveBeenCalledTimes(1);
});
it('restores queue events after subscription setup fails while manual refresh remains available',async()=>{
 vi.useFakeTimers();
 try{
  let emit:(value:MusicQueue)=>void=()=>{};
  const stop=vi.fn();
  const subscribe=vi.fn().mockRejectedValueOnce(Error('Queue subscription unavailable')).mockImplementation(async(update:typeof emit)=>{emit=update;return stop;});
  const services={...productionServices,canUseMedia:true,player:{observe:async()=>()=>{}},music:{...productionServices.music,subscribe,queue:async()=>queue}};
  const view=render(<AppServicesProvider services={services}><PlaybackProvider><Observer/></PlaybackProvider></AppServicesProvider>);
  await act(async()=>{});
  expect(screen.getByRole('alert')).toHaveTextContent('Queue subscription unavailable');
  await act(async()=>fireEvent.click(screen.getByRole('button',{name:'Refresh queue'})));
  expect(screen.getByText('Queue 7')).toBeVisible();
  expect(screen.getByRole('alert')).toHaveTextContent('Queue subscription unavailable');
  expect(subscribe).toHaveBeenCalledTimes(1);
  await act(async()=>vi.advanceTimersByTimeAsync(250));
  await act(async()=>emit({...queue,revision:8}));
  expect(screen.getByText('Queue 8')).toBeVisible();
  expect(screen.queryByRole('alert')).toBeNull();
  await act(async()=>vi.advanceTimersByTimeAsync(5000));
  expect(subscribe).toHaveBeenCalledTimes(2);
  view.unmount();
  expect(stop).toHaveBeenCalledTimes(1);
 }finally{vi.useRealTimers();}
});
it('disposes queue registration that completes after its retry is unmounted',async()=>{
 vi.useFakeTimers();
 try{
  let resolve:(dispose:()=>void)=>void=()=>{};
  const stop=vi.fn(),readQueue=vi.fn(async()=>queue);
  const subscribe=vi.fn().mockRejectedValueOnce(Error('Queue subscription unavailable')).mockImplementation(()=>new Promise<()=>void>(done=>{resolve=done;}));
  const services={...productionServices,canUseMedia:true,player:{observe:async()=>()=>{}},music:{...productionServices.music,subscribe,queue:readQueue}};
  const view=render(<AppServicesProvider services={services}><PlaybackProvider><Observer/></PlaybackProvider></AppServicesProvider>);
  await act(async()=>vi.advanceTimersByTimeAsync(250));
  view.unmount();
  await act(async()=>resolve(stop));
  expect(stop).toHaveBeenCalledTimes(1);
  expect(readQueue).not.toHaveBeenCalled();
 }finally{vi.useRealTimers();}
});
it('flushes the last requested volume after an earlier write fails',async()=>{
 let reject:(reason:Error)=>void=()=>{};
 const controlPlayer=vi.fn().mockImplementationOnce(()=>new Promise((_,fail)=>{reject=fail;})).mockResolvedValue(undefined);
 const services={...productionServices,canUseMedia:true,backend:{...productionServices.backend,controlPlayer,player:async()=>player},player:{observe:async(update:(value:PlayerSnapshot|null)=>void)=>{update(player);return()=>{};}},music:{...productionServices.music,subscribe:async()=>()=>{},queue:async()=>queue}};
 render(<AppServicesProvider services={services}><PlaybackProvider><LocalPlayerControls/></PlaybackProvider></AppServicesProvider>);
 const slider=await screen.findByRole('slider',{name:'Volume'});
 fireEvent.change(slider,{target:{value:'30'}});
 fireEvent.change(slider,{target:{value:'70'}});
 await act(async()=>reject(Error('Volume write failed')));
 expect(controlPlayer.mock.calls).toEqual([['audio-session',{type:'volume',value:30}],['audio-session',{type:'volume',value:70}]]);
});
it('clears a cancelled seek and keeps an ongoing drag bound to the original session',async()=>{
 let emit:(value:PlayerSnapshot|null)=>void=()=>{};
 const controlPlayer=vi.fn(async()=>{});
 const services={...productionServices,canUseMedia:true,backend:{...productionServices.backend,controlPlayer,player:async()=>player},player:{observe:async(update:typeof emit)=>{emit=update;update(player);return()=>{};}},music:{...productionServices.music,subscribe:async()=>()=>{},queue:async()=>queue}};
 render(<AppServicesProvider services={services}><PlaybackProvider><LocalPlayerControls/></PlaybackProvider></AppServicesProvider>);
 const slider=await screen.findByRole('slider',{name:'Seek'});
 fireEvent.pointerDown(slider,{pointerId:1});
 fireEvent.change(slider,{target:{value:'30'}});
 fireEvent.pointerCancel(slider,{pointerId:1});
 expect(slider).toHaveValue('12');
 fireEvent.pointerDown(slider,{pointerId:2});
 fireEvent.change(slider,{target:{value:'35'}});
 await act(async()=>emit({...player,sessionId:'second-session',positionSeconds:5}));
 expect(slider).toHaveValue('5');
 fireEvent.change(slider,{target:{value:'40'}});
 expect(slider).toHaveValue('5');
 fireEvent.pointerUp(slider,{pointerId:2});
 expect(controlPlayer).not.toHaveBeenCalled();
 fireEvent.keyDown(slider,{key:'ArrowRight'});
 fireEvent.change(slider,{target:{value:'6'}});
 await act(async()=>fireEvent.keyUp(slider,{key:'ArrowRight'}));
 expect(controlPlayer).toHaveBeenCalledWith('second-session',{type:'seek',seconds:6});
});
