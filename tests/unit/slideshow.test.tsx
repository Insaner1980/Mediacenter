import {act,render,screen,fireEvent,waitFor} from '@testing-library/react';
import {expect,it,vi} from 'vitest';
import {AppServicesProvider,productionServices} from '../../src/app/AppServices';
import {SlideshowProvider,useSlideshow} from '../../src/features/photos/SlideshowProvider';
import type {SlideshowSnapshot} from '../../src/app/contracts';
const snapshot:SlideshowSnapshot={sessionId:'s',phase:'playing',mediaId:'photo',index:0,total:2,paused:false,intervalSeconds:5,shuffle:false,monitorId:null,error:null,fit:'fit'};
it('does not allow an earlier poll to undo a pause action',async()=>{
 let resolve:(v:SlideshowSnapshot)=>void=()=>{};const photos={...productionServices.photos,snapshot:vi.fn().mockResolvedValueOnce(snapshot).mockImplementationOnce(()=>new Promise(r=>{resolve=r;})).mockResolvedValue({...snapshot,paused:true}),control:vi.fn(async()=>({...snapshot,paused:true}))};
 function View(){const state=useSlideshow();return <><p>{state.snapshot?.paused?'Paused':'Playing'}</p><button onClick={()=>void state.control({type:'pause',paused:true})}>Pause</button></>;}
 render(<AppServicesProvider services={{...productionServices,canUseMedia:true,photos}}><SlideshowProvider><View/></SlideshowProvider></AppServicesProvider>);await waitFor(()=>expect(photos.snapshot).toHaveBeenCalledTimes(2),{timeout:2000});fireEvent.click(screen.getByText('Pause'));await act(async()=>resolve(snapshot));await screen.findByText('Paused');expect(photos.control).toHaveBeenCalledWith({type:'pause',paused:true});
});
it.each([[10000,true],[10001,false]] as const)('checks the slideshow limit across cursor pages (%s)',async(count,allowed)=>{
 const {PhotosScreen}=await import('../../src/features/photos/PhotosScreen');const user=(await import('@testing-library/user-event')).default.setup();
 const item={id:'photo',sourceId:'s',kind:'photo' as const,displayName:'Photo',availability:'available',durationMs:null,resumePositionMs:null,thumbnailId:null};
 const start=vi.fn(async(request:import('../../src/app/contracts').SlideshowRequest)=>({...snapshot,total:request.mediaIds.length}));const query=vi.fn(async(_kind:unknown,_source:unknown,_query:unknown,cursor:string|null)=>{const offset=Number(cursor??0);return {items:Array.from({length:Math.min(50,count-offset)},(_,i)=>({...item,id:`photo-${offset+i}`})),nextCursor:offset+50<count?String(offset+50):null};});
 render(<PhotosScreen api={{...productionServices.photos,snapshot:async()=>({...snapshot,sessionId:null}),start}} libraryApi={{...productionServices.backend,sources:async()=>[],outputs:async()=>({monitors:[],audioSinks:[],computerMonitorId:null,audioError:null}),queryMedia:query,thumbnail:async()=>({mime:'image/png',base64:'AAAA'})}}/>);
 await screen.findAllByRole('button',{name:'Photo'});await user.click(screen.getByRole('button',{name:'Slideshow'}));await waitFor(()=>expect(screen.getByRole('button',{name:'Start slideshow'})).toBeEnabled());await user.click(screen.getByRole('button',{name:'Start slideshow'}));
 if(allowed){await waitFor(()=>expect(start).toHaveBeenCalledTimes(1));expect(start.mock.calls[0][0]).toEqual({mediaIds:Array.from({length:count},(_,i)=>`photo-${i}`),monitorId:null,intervalSeconds:5,shuffle:false});}else{await screen.findByText('Narrow the source or search to at most 10,000 photos.');expect(start).not.toHaveBeenCalled();}
});
it('accepts whole-second boundaries and rejects zero, fractional and oversized intervals',async()=>{
 const {PhotosScreen}=await import('../../src/features/photos/PhotosScreen');const user=(await import('@testing-library/user-event')).default.setup();
 render(<PhotosScreen api={{...productionServices.photos,snapshot:async()=>({...snapshot,sessionId:null})}} libraryApi={{...productionServices.backend,sources:async()=>[],outputs:async()=>({monitors:[],audioSinks:[],computerMonitorId:null,audioError:null}),queryMedia:async()=>({items:[],nextCursor:null})}}/>);await user.click(screen.getByRole('button',{name:'Slideshow'}));await waitFor(()=>expect(screen.getByRole('button',{name:'Start slideshow'})).toBeEnabled());
 for(const value of [0,61,1.5]){fireEvent.change(screen.getByLabelText('Seconds per photo'),{target:{value:String(value)}});expect(screen.getByRole('button',{name:'Start slideshow'})).toBeDisabled();}for(const value of [1,60]){fireEvent.change(screen.getByLabelText('Seconds per photo'),{target:{value:String(value)}});expect(screen.getByRole('button',{name:'Start slideshow'})).toBeEnabled();}
});
