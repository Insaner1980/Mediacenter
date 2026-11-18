import type {HomeSnapshot} from '../../app/contracts';
import {Dialog} from '../../ui/Dialog';
import {OutputSettings} from './OutputSettings';
import './outputs.css';
export function OutputPanel({open,onRequestClose,snapshot}:{open:boolean;onRequestClose:()=>void;snapshot:HomeSnapshot}){
 const session=snapshot.sessions.find(s=>s.kind==='streaming'||s.kind.startsWith('local-'));
 return <Dialog open={open} title="Picture & sound" variant="popover" onRequestClose={onRequestClose}><OutputSettings output={snapshot.outputs} session={session} compact/></Dialog>;
}
