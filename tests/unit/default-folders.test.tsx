import {act,fireEvent,render,screen,waitFor} from '@testing-library/react';
import {expect,it,vi} from 'vitest';
import {backend} from '../../src/app/backend';
import {AppServicesProvider,productionServices} from '../../src/app/AppServices';
import {SourceManagerProvider} from '../../src/features/library/SourceManagerProvider';
import {useMediaLibrary} from '../../src/features/library/useMediaLibrary';

function setup(overrides:Partial<typeof backend>={}){
 const api={...backend,ensureDefaultSource:vi.fn().mockResolvedValue('pictures'),sources:vi.fn().mockResolvedValue([{id:'pictures',label:'Pictures',displayPath:'/home/test/Pictures',kinds:['photo'],availability:'available',removable:false}]),queryMedia:vi.fn().mockResolvedValue({items:[],nextCursor:null}),scanSource:vi.fn().mockResolvedValue(undefined),chooseSource:vi.fn().mockResolvedValue(null),addSource:vi.fn().mockResolvedValue('usb'),...overrides};
 function View(){const library=useMediaLibrary('photo');return <><p>Selected {library.sourceId}</p><button onClick={()=>library.setSourceId('manual')}>Select manual</button><button onClick={()=>void library.openFolder()}>Open folder</button><button onClick={library.refresh}>Refresh</button>{library.error&&<p role="alert">{library.error}</p>}</>;}
 render(<AppServicesProvider services={{...productionServices,canUseMedia:true,backend:api,observeSources:async()=>()=>{}}}><SourceManagerProvider><View/></SourceManagerProvider></AppServicesProvider>);return api;
}
it('selects and scans Pictures automatically before querying that folder',async()=>{const api=setup();await screen.findByText('Selected pictures');await waitFor(()=>expect(api.scanSource).toHaveBeenCalledWith('pictures'));await waitFor(()=>expect(api.queryMedia).toHaveBeenCalledWith('photo','pictures','',null));expect(vi.mocked(api.queryMedia).mock.calls.every(call=>call[1]==='pictures')).toBe(true);});
it('never replaces a user choice with a delayed default folder',async()=>{let resolve:(id:string|null)=>void=()=>{};const api=setup({ensureDefaultSource:()=>new Promise(r=>{resolve=r;})});fireEvent.click(screen.getByText('Select manual'));await act(async()=>resolve('pictures'));expect(screen.getByText('Selected manual')).toBeVisible();expect(api.scanSource).not.toHaveBeenCalledWith('pictures');});
it('opens a chosen external folder in one step and refresh rescans it',async()=>{const api=setup({chooseSource:vi.fn().mockResolvedValue({grantId:'grant',displayPath:'/media/USB/Photos',expiresInSeconds:600})});await screen.findByText('Selected pictures');fireEvent.click(screen.getByText('Open folder'));await screen.findByText('Selected usb');expect(api.addSource).toHaveBeenCalledWith('grant',['photo']);await waitFor(()=>expect(api.scanSource).toHaveBeenCalledWith('usb'));await waitFor(()=>expect(api.queryMedia).toHaveBeenLastCalledWith('photo','usb','',null));const count=vi.mocked(api.scanSource).mock.calls.length;fireEvent.click(screen.getByText('Refresh'));await waitFor(()=>expect(vi.mocked(api.scanSource).mock.calls.length).toBe(count+1));});
it('canceling the picker preserves the folder and creates no source',async()=>{const api=setup();await screen.findByText('Selected pictures');fireEvent.click(screen.getByText('Open folder'));await waitFor(()=>expect(api.chooseSource).toHaveBeenCalled());expect(api.addSource).not.toHaveBeenCalled();expect(screen.getByText('Selected pictures')).toBeVisible();});
it('shows automatic scanning failure in the library',async()=>{setup({scanSource:vi.fn().mockRejectedValue(Error('USB disconnected'))});expect(await screen.findByRole('alert')).toHaveTextContent('USB disconnected');});
it('queues one follow-up scan when a second view enables media while a scan is running',async()=>{
 const {useSources}=await import('../../src/features/library/SourceManagerProvider');const finishes:Array<()=>void>=[];
 const scanSource=vi.fn(()=>new Promise<void>(resolve=>{finishes.push(resolve);}));
 function View(){const state=useSources();return <button onClick={()=>void state.scan('shared')}>Scan shared folder</button>;}
 render(<AppServicesProvider services={{...productionServices,canUseMedia:true,backend:{...backend,sources:async()=>[],scanSource},observeSources:async()=>()=>{}}}><SourceManagerProvider><View/></SourceManagerProvider></AppServicesProvider>);
 fireEvent.click(screen.getByText('Scan shared folder'));fireEvent.click(screen.getByText('Scan shared folder'));fireEvent.click(screen.getByText('Scan shared folder'));expect(scanSource).toHaveBeenCalledTimes(1);await act(async()=>finishes[0]());await waitFor(()=>expect(scanSource).toHaveBeenCalledTimes(2));await act(async()=>finishes[1]());expect(scanSource).toHaveBeenCalledTimes(2);
});
