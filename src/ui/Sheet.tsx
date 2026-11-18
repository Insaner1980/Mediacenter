import {Dialog,type DialogProps} from './Dialog';
export type SheetProps=Omit<DialogProps,'variant'>;
export function Sheet(props:SheetProps){return <Dialog {...props} variant="sheet"/>;}
