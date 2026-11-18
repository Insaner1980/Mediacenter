import {Icon} from '../../ui/Icons';
import type {MediaLibraryState} from './useMediaLibrary';
export function LibraryFolderActions({library}:{library:MediaLibraryState}){
 return <div className="folder-actions"><button className="text-button" disabled={library.opening} onClick={()=>void library.openFolder()}><Icon name="folder"/>{library.opening?'Opening folder':'Open folder'}</button></div>;
}
