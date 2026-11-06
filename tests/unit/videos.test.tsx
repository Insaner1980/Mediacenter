import {render,screen,waitFor} from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import {expect,it,vi} from 'vitest';
import {VideosScreen} from '../../src/features/videos/VideosScreen';
import {AppServicesProvider,productionServices} from '../../src/app/AppServices';
import {SourceManagerProvider} from '../../src/features/library/SourceManagerProvider';
import type {MediaItem} from '../../src/app/contracts';
const item:MediaItem={id:'video-a',displayName:'Film',kind:'video',sourceId:'source',availability:'available',durationMs:60000,resumePositionMs:12000,thumbnailId:null};
it('plays from the thumbnail, title and duration without a preview and blocks offline play',async()=>{
 const play=vi.fn(async()=>{}),api={...productionServices.backend,sources:async()=>[],thumbnail:async()=>({mime:'image/png',base64:'AAAA'}),queryMedia:async()=>({items:[item,{...item,id:'offline',displayName:'Offline recording',availability:'offline'}],nextCursor:null})};
 const user=userEvent.setup();render(<AppServicesProvider services={{...productionServices,canUseMedia:true,backend:api}}><SourceManagerProvider><VideosScreen onPlay={play}/></SourceManagerProvider></AppServicesProvider>);
 await waitFor(()=>expect(screen.getByRole('button',{name:'Play Film'})).toBeEnabled());
 await user.click(screen.getByText('Film'));expect(play).toHaveBeenLastCalledWith(item,true);expect(screen.queryByRole('dialog')).not.toBeInTheDocument();
 await user.click(screen.getAllByText('1:00')[0]);expect(play).toHaveBeenCalledTimes(2);
 await user.click(screen.getByRole('button',{name:'Play Film'}));expect(play).toHaveBeenCalledTimes(3);
 await user.click(screen.getByRole('button',{name:'Play Film from start'}));expect(play).toHaveBeenLastCalledWith(item,false);expect(screen.getByRole('button',{name:'Play Offline recording'})).toBeDisabled();
});
