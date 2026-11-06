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
