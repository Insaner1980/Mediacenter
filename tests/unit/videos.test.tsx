import {act,render,screen,waitFor} from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import {expect,it,vi} from 'vitest';
import {VideosScreen} from '../../src/features/videos/VideosScreen';
import {AppServicesProvider,productionServices} from '../../src/app/AppServices';
import {SourceManagerProvider} from '../../src/features/library/SourceManagerProvider';
import type {MediaItem} from '../../src/app/contracts';
const item:MediaItem={id:'video-a',displayName:'Film',kind:'video',sourceId:'source',availability:'available',durationMs:60000,resumePositionMs:12000,thumbnailId:null};
it('plays from the thumbnail, title and duration without a preview and blocks offline play',async()=>{
 const play=vi.fn(async()=>{}),api={...productionServices.backend,ensureDefaultSource:async()=>null,sources:async()=>[],thumbnail:async()=>({mime:'image/png',base64:'AAAA'}),queryMedia:async()=>({items:[item,{...item,id:'offline',displayName:'Offline recording',availability:'offline'}],nextCursor:null})};
 const user=userEvent.setup();render(<AppServicesProvider services={{...productionServices,canUseMedia:true,backend:api,observeSources:async()=>()=>{}}}><SourceManagerProvider><VideosScreen onPlay={play}/></SourceManagerProvider></AppServicesProvider>);
 await act(async()=>{});
 await waitFor(()=>expect(screen.getByRole('button',{name:'Film, continue at 0:12'})).toBeEnabled());
 await user.click(screen.getByText('Film'));expect(play).toHaveBeenLastCalledWith(item,true);expect(screen.queryByRole('dialog')).not.toBeInTheDocument();
 await user.click(screen.getAllByText('1:00')[0]);expect(play).toHaveBeenCalledTimes(2);
 await user.click(screen.getByRole('button',{name:'Film, continue at 0:12'}));expect(play).toHaveBeenCalledTimes(3);
 await user.click(screen.getByRole('button',{name:'Play Film from start'}));expect(play).toHaveBeenLastCalledWith(item,false);expect(screen.getByRole('button',{name:'Play Offline recording'})).toBeDisabled();
});
it('keeps the video results usable when the remote repeats a search',async()=>{
 const queryMedia=vi.fn(async()=>({items:[{...item,resumePositionMs:null}],nextCursor:null}));
 const api={...productionServices.backend,ensureDefaultSource:async()=>null,sources:async()=>[],queryMedia};
 render(<AppServicesProvider services={{...productionServices,canUseMedia:true,backend:api,observeSources:async()=>()=>{}}}><SourceManagerProvider><VideosScreen onPlay={async()=>{}}/></SourceManagerProvider></AppServicesProvider>);
 const input=screen.getByRole('textbox',{name:'Search videos'});
 const search=()=>input.dispatchEvent(new CustomEvent('remote-search',{detail:'Film'}));
 act(search);
 await waitFor(()=>expect(screen.getByRole('button',{name:'Play Film'})).toBeEnabled());
 act(search);
 await waitFor(()=>expect(screen.getByRole('button',{name:'Play Film'})).toBeEnabled());
});
it('returns to the previous results when the next page fails',async()=>{
 const queryMedia=vi.fn<typeof productionServices.backend.queryMedia>(async(_kind,_source,_query,cursor)=>{
  if(cursor==='after-film')throw Error('Page unavailable');
  return {items:[{...item,resumePositionMs:null}],nextCursor:'after-film'};
 });
 const api={...productionServices.backend,ensureDefaultSource:async()=>null,sources:async()=>[],queryMedia};
 render(<AppServicesProvider services={{...productionServices,canUseMedia:true,backend:api,observeSources:async()=>()=>{}}}><SourceManagerProvider><VideosScreen onPlay={async()=>{}}/></SourceManagerProvider></AppServicesProvider>);
 const user=userEvent.setup();
 await user.click(await screen.findByRole('button',{name:'Next page'}));
 expect(await screen.findByRole('alert')).toHaveTextContent('Page unavailable');
 expect(screen.getByRole('button',{name:'Next page'})).toBeDisabled();
 await user.click(screen.getByRole('button',{name:'Previous page'}));
 await waitFor(()=>expect(screen.getByRole('button',{name:'Play Film'})).toBeEnabled());
});
