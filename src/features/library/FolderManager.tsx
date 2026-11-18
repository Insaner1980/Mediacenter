import type {MediaKind} from '../../app/contracts';
import {Sheet} from '../../ui/Sheet';
import {SourcePicker} from './SourcePicker';
export function FolderManager({open,kind,onRequestClose}:{open:boolean;kind?:MediaKind;onRequestClose:()=>void}){return <Sheet open={open} title="Media folders" onRequestClose={onRequestClose}><SourcePicker kind={kind}/></Sheet>;}
