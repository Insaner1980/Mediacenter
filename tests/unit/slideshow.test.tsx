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
