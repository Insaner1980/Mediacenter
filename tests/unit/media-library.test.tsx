import {act,fireEvent,render,screen,waitFor} from '@testing-library/react';
import {expect,it,vi} from 'vitest';
import {backend} from '../../src/app/backend';
import type {MediaPage} from '../../src/app/contracts';
import {useMediaLibrary} from '../../src/features/library/useMediaLibrary';
it('discards an old search response after changing query and source',async()=>{
 let resolve:(page:MediaPage)=>void=()=>{};
 const api={...backend,sources:async()=>[],queryMedia:vi.fn().mockImplementationOnce(()=>new Promise<MediaPage>(r=>{resolve=r;})).mockResolvedValue({items:[{id:'b',displayName:'New result'}],nextCursor:null})};
 function View(){const state=useMediaLibrary('video',api);return <><input aria-label="Search" value={state.query} onChange={e=>{state.setSourceId('source-b');state.setQuery(e.target.value);}}/>{state.items.map(i=><p key={i.id}>{i.displayName}</p>)}</>;}
 render(<View/>);await waitFor(()=>expect(api.queryMedia).toHaveBeenCalled());fireEvent.change(screen.getByLabelText('Search'),{target:{value:'new query'}});await screen.findByText('New result');await act(async()=>resolve({items:[{id:'old',displayName:'Old result'}] as MediaPage['items'],nextCursor:null}));
 expect(screen.queryByText('Old result')).not.toBeInTheDocument();expect(api.queryMedia).toHaveBeenLastCalledWith('video','source-b','new query',null);
});
it('invalidates a pending query when a source changes availability without losing its selection',async()=>{
 let emit:(sources:import('../../src/app/contracts').MediaSource[])=>void=()=>{};let finish:(page:MediaPage)=>void=()=>{};
 const source={id:'source-a',label:'USB',displayPath:'/media/usb',kinds:['video' as const],availability:'available',removable:true};
 const {AppServicesProvider,productionServices}=await import('../../src/app/AppServices');const {SourceManagerProvider}=await import('../../src/features/library/SourceManagerProvider');
 const item={id:'film',displayName:'Film',kind:'video' as const,sourceId:'source-a',availability:'available',durationMs:null,resumePositionMs:null,thumbnailId:null};let calls=0;
 const api={...backend,sources:async()=>[source],queryMedia:vi.fn(async()=>{calls++;if(calls===2)return new Promise<MediaPage>(r=>{finish=r;});return {items:[{...item,availability:calls>2?'offline':'available'}],nextCursor:null};})};
 const services={...productionServices,canUseMedia:true,backend:api,observeSources:async(update:typeof emit)=>{emit=update;return()=>{};}};
 function View(){const state=useMediaLibrary('video');return <><button onClick={()=>state.setSourceId('source-a')}>Select source</button><p>Selected {state.sourceId}</p><p>Source {state.sources[0]?.availability}</p>{state.items.map(i=><p key={i.id}>Item {i.availability}</p>)}</>;}
 render(<AppServicesProvider services={services}><SourceManagerProvider><View/></SourceManagerProvider></AppServicesProvider>);await screen.findByText('Item available');fireEvent.click(screen.getByText('Select source'));await waitFor(()=>expect(api.queryMedia).toHaveBeenCalledTimes(2));await act(async()=>emit([{...source,availability:'offline'}]));await screen.findByText('Source offline');await act(async()=>finish({items:[item],nextCursor:null}));await screen.findByText('Item offline');expect(screen.getByText('Selected source-a')).toBeVisible();
});
it('keeps stale results and cursors disabled when the new filter fails',async()=>{
 const api={...backend,sources:async()=>[],queryMedia:vi.fn().mockResolvedValueOnce({items:[{id:'a',displayName:'Old film'}],nextCursor:'old-next'}).mockRejectedValue(Error('Source offline'))};
 function View(){const state=useMediaLibrary('video',api);return <><button onClick={()=>state.setSourceId('source-b')}>Change source</button><button disabled={state.loading||state.stale} onClick={state.nextPage}>Next</button>{state.items.map(i=><button key={i.id} disabled={state.loading||state.stale}>Play {i.displayName}</button>)}{state.error&&<p role="alert">{state.error}</p>}</>;}
 render(<View/>);await screen.findByText('Play Old film');fireEvent.click(screen.getByText('Change source'));await screen.findByRole('alert');expect(screen.getByText('Play Old film')).toBeDisabled();expect(screen.getByText('Next')).toBeDisabled();
});
it('keeps a source event over a delayed initial list response',async()=>{
 const {AppServicesProvider,productionServices}=await import('../../src/app/AppServices');const {SourceManagerProvider,useSources}=await import('../../src/features/library/SourceManagerProvider');let emit:(v:import('../../src/app/contracts').MediaSource[])=>void=()=>{};let resolve:(v:import('../../src/app/contracts').MediaSource[])=>void=()=>{};
 const source={id:'s',label:'USB',displayPath:'/usb',kinds:['video' as const],availability:'offline',removable:true};const list=vi.fn(()=>new Promise<import('../../src/app/contracts').MediaSource[]>(r=>{resolve=r;}));const services={...productionServices,canUseMedia:true,backend:{...backend,sources:list},observeSources:async(update:typeof emit)=>{emit=update;return()=>{};}};
 function View(){const state=useSources();return <p>{state.sources[0]?.availability}</p>;}
 render(<AppServicesProvider services={services}><SourceManagerProvider><View/></SourceManagerProvider></AppServicesProvider>);await waitFor(()=>expect(list).toHaveBeenCalled());await act(async()=>emit([source]));await act(async()=>resolve([{...source,availability:'available'}]));expect(screen.getByText('offline')).toBeVisible();
});
it('publishes a completed scan when an identical event supersedes its source list',async()=>{
 const {AppServicesProvider,productionServices}=await import('../../src/app/AppServices');
 const {SourceManagerProvider,useSources}=await import('../../src/features/library/SourceManagerProvider');
 const source={id:'s',label:'USB',displayPath:'/usb',kinds:['video' as const],availability:'available',removable:true};
 let emit:(value:typeof source[])=>void=()=>{},resolve:(value:typeof source[])=>void=()=>{};
 const list=vi.fn().mockResolvedValueOnce([source]).mockImplementation(()=>new Promise(r=>{resolve=r;}));
 const services={...productionServices,canUseMedia:true,backend:{...backend,sources:list,scanSource:async()=>{}},observeSources:async(update:typeof emit)=>{emit=update;return()=>{};}};
 function View(){const state=useSources();return <><button onClick={()=>void state.scan('s')}>Scan USB</button><p>Revision {state.revision}</p></>;}
 render(<AppServicesProvider services={services}><SourceManagerProvider><View/></SourceManagerProvider></AppServicesProvider>);
 await screen.findByText('Revision 1');
 fireEvent.click(screen.getByText('Scan USB'));
 await waitFor(()=>expect(list).toHaveBeenCalledTimes(2));
 await act(async()=>emit([source]));
 await act(async()=>resolve([source]));
 expect(screen.getByText('Revision 2')).toBeVisible();
});
