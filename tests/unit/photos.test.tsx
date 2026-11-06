import {act,render,screen,waitFor} from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import {it,expect,vi} from 'vitest';
import {PresentationScreen} from '../../src/features/photos/PresentationScreen';
import {PhotosScreen} from '../../src/features/photos/PhotosScreen';
import {backend} from '../../src/app/backend';
import {photosApi} from '../../src/features/photos/api';
const snapshot={sessionId:'s',phase:'paused',mediaId:'a',index:0,total:1,paused:true,intervalSeconds:5,shuffle:false,monitorId:'HDMI-1',error:'Display disconnected. Continue on the computer when ready.',fit:'fit'} as const;
it('presentation accepts only assigned decoded raster images',async()=>{
 render(<PresentationScreen api={{...photosApi,frame:vi.fn().mockResolvedValue({snapshot,image:{image:{mime:'image/svg+xml',base64:'PHN2Zy8+'},width:100,height:100,modifiedAtMs:null}})}}/>);
 await act(async()=>{await Promise.resolve();});
 await waitFor(()=>expect(screen.getByRole('status')).toHaveTextContent('Waiting for a photo'));
 expect(screen.queryByRole('img')).not.toBeInTheDocument();
});
it('disconnected display recovery remains an explicit computer action',async()=>{
 const control=vi.fn().mockResolvedValue({...snapshot,phase:'playing',paused:false,error:null,monitorId:'eDP-1'});
 render(<PhotosScreen api={{...photosApi,snapshot:vi.fn().mockResolvedValue(snapshot),control}} libraryApi={{...backend,sources:vi.fn().mockResolvedValue([]),queryMedia:vi.fn().mockResolvedValue({items:[],nextCursor:null}),outputs:vi.fn().mockResolvedValue({monitors:[],audioSinks:[],computerMonitorId:null})}}/>);
 await userEvent.click(await screen.findByRole('button',{name:'Show slideshow controls'}));const button=await screen.findByRole('button',{name:'Continue on computer'});expect(control).not.toHaveBeenCalled();await userEvent.click(button);expect(control).toHaveBeenCalledWith({type:'continue_on_computer'});
});

it('presentation shows the assigned safe image with no privileged controls',async()=>{
 render(<PresentationScreen api={{...photosApi,frame:vi.fn().mockResolvedValue({snapshot,image:{image:{mime:'image/jpeg',base64:'/9j/'},width:100,height:50,modifiedAtMs:null}})}}/>);
 const image=await screen.findByRole('img',{name:'Photo 1 of 1'});expect(image).toHaveAttribute('src','data:image/jpeg;base64,/9j/');expect(screen.queryByRole('button')).not.toBeInTheDocument();
});
it('opening a photo brings the full viewer into keyboard focus',async()=>{
 const item={id:'photo',sourceId:'source',kind:'photo' as const,displayName:'Holiday.jpg',availability:'available',modifiedAtMs:0,durationMs:null,thumbnailId:null,resumePositionMs:null};
 render(<PhotosScreen api={{...photosApi,snapshot:vi.fn().mockResolvedValue({...snapshot,sessionId:null}),image:vi.fn().mockResolvedValue({image:{mime:'image/jpeg',base64:'/9j/'},width:100,height:50,modifiedAtMs:0})}} libraryApi={{...backend,sources:vi.fn().mockResolvedValue([]),queryMedia:vi.fn().mockResolvedValue({items:[item],nextCursor:null}),thumbnail:vi.fn().mockResolvedValue({mime:'image/jpeg',base64:'/9j/'}),outputs:vi.fn().mockResolvedValue({monitors:[],audioSinks:[],computerMonitorId:null,audioError:null})}}/>);
 await userEvent.click(await screen.findByRole('button',{name:/Holiday.jpg/}));
 expect(await screen.findByRole('dialog',{name:'Holiday.jpg'})).toContainElement(document.activeElement as HTMLElement);
 expect(await screen.findByRole('img',{name:'Holiday.jpg'})).toHaveAttribute('src','data:image/jpeg;base64,/9j/');
 expect(screen.queryByRole('button',{name:'Close photo'})).not.toBeInTheDocument();
 await userEvent.click(screen.getByRole('button',{name:'Close'}));
 expect(screen.queryByRole('dialog')).not.toBeInTheDocument();
});
