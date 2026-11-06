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
