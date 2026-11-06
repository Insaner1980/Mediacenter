import {Icon} from '../../ui/Icons';
import {IconButton} from '../../ui/IconButton';
import type {MediaLibraryState} from './useMediaLibrary';
export function LibraryFolderActions({library,onManage}:{library:MediaLibraryState;onManage:()=>void}){
 return <div className="folder-actions"><button className="text-button" disabled={library.opening} onClick={()=>void library.openFolder()}><Icon name="folder"/>{library.opening?'Opening folder':'Open folder'}</button><IconButton label="Manage folders" icon="settings" onClick={onManage}/></div>;
}
