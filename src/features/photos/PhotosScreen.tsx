import {useState} from 'react';
import {backend} from '../../app/backend';
import type {MediaItem} from '../../app/contracts';
import {AppServicesProvider,useAppServices} from '../../app/AppServices';
import {SourceManagerProvider} from '../library/SourceManagerProvider';
import {useMediaLibrary} from '../library/useMediaLibrary';
import {LibraryToolbar} from '../library/LibraryToolbar';
import {LibraryPagination} from '../library/LibraryPagination';
import {MediaThumbnail} from '../library/MediaThumbnail';
import {FolderManager} from '../library/FolderManager';
import {PageHeader} from '../../ui/PageHeader';
import {Icon} from '../../ui/Icons';
import {photosApi} from './api';
import {PhotoViewer} from './PhotoViewer';
import {SlideshowProvider,useOptionalSlideshow,useSlideshow} from './SlideshowProvider';
import {SlideshowControls} from './SlideshowControls';
import './photos.css';
export function PhotosScreen({api,libraryApi}:{api?:typeof photosApi;libraryApi?:typeof backend}){const services=useAppServices(),shared=useOptionalSlideshow();if(shared)return <PhotosContent/>;return <AppServicesProvider services={{...services,photos:api??services.photos,backend:libraryApi??services.backend,canUseMedia:true}}><SourceManagerProvider><SlideshowProvider><PhotosContent/></SlideshowProvider></SourceManagerProvider></AppServicesProvider>;}
function PhotosContent(){
 const library=useMediaLibrary('photo'),slideshow=useSlideshow(),[folders,setFolders]=useState(false),[setup,setSetup]=useState(false),[selected,setSelected]=useState<MediaItem|null>(null);
 return <><PageHeader title="Photos" actions={<><button className="text-button" onClick={()=>setFolders(true)}><Icon name="folder"/>Manage folders</button><button className="control primary" onClick={()=>setSetup(true)}><Icon name="play"/>{slideshow.snapshot?.sessionId?'Show slideshow controls':'Slideshow'}<Icon name="chevron"/></button></>}/><LibraryToolbar kind="photo" {...library} onQueryChange={library.setQuery} onSourceChange={library.setSourceId} onRefresh={library.refresh}/>{library.error&&<p role="alert">{library.error}</p>}{library.loading&&<p role="status">Loading photos</p>}<div className="media-grid photo-grid" data-testid="photo-grid" aria-busy={library.loading}>{library.items.map(item=><article key={item.id}><button className="thumbnail-button photo-card" disabled={library.loading||item.availability!=='available'} onClick={()=>setSelected(item)}><MediaThumbnail item={item}/><strong title={item.displayName}>{item.displayName}</strong></button>{item.availability!=='available'&&<span>{item.availability}</span>}</article>)}</div>{!library.loading&&!library.items.length&&<div className="library-empty"><p>{library.sources.length?'No matching photos':'No photos yet'}</p>{!library.sources.length?<button className="control" onClick={()=>setFolders(true)}>Add folder</button>:library.query&&<button className="control" onClick={()=>library.setQuery('')}>Clear search</button>}</div>}<LibraryPagination library={library}/><FolderManager open={folders} kind="photo" onRequestClose={()=>setFolders(false)}/><SlideshowControls open={setup} onRequestClose={()=>setSetup(false)} library={library}/>{selected&&<PhotoViewer item={library.items.find(i=>i.id===selected.id)??selected} onRequestClose={()=>setSelected(null)}/>}</>;
}
export default PhotosScreen;
