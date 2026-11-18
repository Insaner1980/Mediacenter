import {act,render,screen,waitFor} from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import {useState} from 'react';
import {expect,it,vi} from 'vitest';
import {AppServicesProvider,productionServices} from '../../src/app/AppServices';
import {SourceManagerProvider} from '../../src/features/library/SourceManagerProvider';
import {FolderManager} from '../../src/features/library/FolderManager';
it('keeps scan cancellation available after closing and reopening the sheet',async()=>{
 let done:()=>void=()=>{};const api={...productionServices.backend,sources:async()=>[{id:'source-a',label:'Music',displayPath:'/media/music',kinds:['audio' as const],availability:'available',removable:false}],scanSource:vi.fn(()=>new Promise<void>(r=>{done=r;})),cancelScan:vi.fn(async()=>{})};
 function View(){const [open,setOpen]=useState(true);return <><button onClick={()=>setOpen(true)}>Folders</button><FolderManager open={open} kind="audio" onRequestClose={()=>setOpen(false)}/></>;}
 const user=userEvent.setup();render(<AppServicesProvider services={{...productionServices,canUseMedia:true,backend:api}}><SourceManagerProvider><View/></SourceManagerProvider></AppServicesProvider>);
 await user.click(await screen.findByRole('button',{name:'Scan Music'}));await waitFor(()=>expect(api.scanSource).toHaveBeenCalled());await user.click(screen.getByRole('button',{name:'Close'}));await user.click(screen.getByRole('button',{name:'Folders'}));await user.click(screen.getByRole('button',{name:'Cancel scan Music'}));expect(api.cancelScan).toHaveBeenCalledWith('source-a');await act(async()=>done());
});
it('adds only the chosen kinds using the grant, not its display path',async()=>{
 const api={...productionServices.backend,sources:async()=>[],chooseSource:async()=>({grantId:'grant-123',displayPath:'/photos',expiresInSeconds:60}),addSource:vi.fn(async()=> 'source-a')};const user=userEvent.setup();
 render(<AppServicesProvider services={{...productionServices,canUseMedia:true,backend:api}}><SourceManagerProvider><FolderManager open kind="photo" onRequestClose={()=>{}}/></SourceManagerProvider></AppServicesProvider>);
 await user.click(screen.getByRole('button',{name:'Add folder'}));await user.click(screen.getByRole('button',{name:'Add selected folder'}));expect(api.addSource).toHaveBeenCalledWith('grant-123',['photo']);
});
it('requires the current media type when adding a folder from a filtered picker',async()=>{
 const addSource=vi.fn(async()=> 'source-a');
 const api={...productionServices.backend,sources:async()=>[],chooseSource:async()=>({grantId:'grant-123',displayPath:'/photos',expiresInSeconds:60}),addSource};
 const user=userEvent.setup();
 render(<AppServicesProvider services={{...productionServices,canUseMedia:true,backend:api}}><SourceManagerProvider><FolderManager open kind="photo" onRequestClose={()=>{}}/></SourceManagerProvider></AppServicesProvider>);
 await user.click(screen.getByRole('button',{name:'Add folder'}));
 await user.click(screen.getByRole('checkbox',{name:'Photos'}));
 await user.click(screen.getByRole('checkbox',{name:'Music'}));
 expect(screen.getByRole('button',{name:'Add selected folder'})).toBeDisabled();
 await user.click(screen.getByRole('checkbox',{name:'Photos'}));
 await user.click(screen.getByRole('button',{name:'Add selected folder'}));
 expect(addSource).toHaveBeenCalledWith('grant-123',['audio','photo']);
});
it('retries the folder list after a successful add without adding it again',async()=>{
 const source={id:'source-a',label:'Pictures',displayPath:'/photos',kinds:['photo' as const],availability:'available',removable:false};
 const sources=vi.fn().mockResolvedValueOnce([]).mockRejectedValueOnce(Error('Source list unavailable')).mockResolvedValue([source]);
 const addSource=vi.fn(async()=>source.id);
 const api={...productionServices.backend,sources,chooseSource:async()=>({grantId:'grant-123',displayPath:'/photos',expiresInSeconds:60}),addSource};
 const user=userEvent.setup();
 render(<AppServicesProvider services={{...productionServices,canUseMedia:true,backend:api}}><SourceManagerProvider><FolderManager open kind="photo" onRequestClose={()=>{}}/></SourceManagerProvider></AppServicesProvider>);
 await waitFor(()=>expect(sources).toHaveBeenCalledTimes(1));
 await user.click(screen.getByRole('button',{name:'Add folder'}));
 await user.click(screen.getByRole('button',{name:'Add selected folder'}));
 expect(await screen.findByRole('status')).toHaveTextContent('Folder added');
 expect(screen.queryByRole('button',{name:'Scan Pictures'})).not.toBeInTheDocument();
 await user.click(screen.getByRole('button',{name:'Retry folder list'}));
 expect(await screen.findByRole('button',{name:'Scan Pictures'})).toBeVisible();
 expect(addSource).toHaveBeenCalledTimes(1);
});
it('closes removal confirmation when the source was removed but the list fails to refresh',async()=>{
 const source={id:'source-a',label:'Pictures',displayPath:'/photos',kinds:['photo' as const],availability:'available',removable:false};
 const sources=vi.fn().mockResolvedValueOnce([source]).mockRejectedValue(Error('Source list unavailable'));
 const api={...productionServices.backend,sources,removeSource:vi.fn(async()=>{})};
 const user=userEvent.setup();
 render(<AppServicesProvider services={{...productionServices,canUseMedia:true,backend:api}}><SourceManagerProvider><FolderManager open kind="photo" onRequestClose={()=>{}}/></SourceManagerProvider></AppServicesProvider>);
 await user.click(await screen.findByRole('button',{name:'Remove Pictures from library'}));
 await user.click(screen.getByRole('button',{name:'Remove from library'}));
 expect(await screen.findByRole('alert')).toHaveTextContent('Source list unavailable');
 expect(screen.queryByRole('dialog',{name:'Remove from library?'})).not.toBeInTheDocument();
 expect(screen.queryByRole('button',{name:'Remove Pictures from library'})).not.toBeInTheDocument();
});
