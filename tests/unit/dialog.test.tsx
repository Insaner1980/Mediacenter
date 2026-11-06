import {useState} from 'react';
import {render,screen} from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import {expect,it,vi} from 'vitest';
import {Sheet} from '../../src/ui/Sheet';
it('contains keyboard focus, requests closure and restores the opener',async()=>{
 const close=vi.fn();
 function Example(){const [open,setOpen]=useState(false);return <><button onClick={()=>setOpen(true)}>Open</button><Sheet open={open} title="Folders" onRequestClose={()=>{close();setOpen(false);}}><button>First</button><button>Last</button></Sheet></>;}
 const user=userEvent.setup();render(<Example/>);await user.click(screen.getByText('Open'));
 expect(screen.getByRole('dialog')).toContainElement(document.activeElement as HTMLElement);
 for(let i=0;i<6;i++) {await user.tab();expect(screen.getByRole('dialog')).toContainElement(document.activeElement as HTMLElement);}
 await user.tab({shift:true});expect(screen.getByRole('dialog')).toContainElement(document.activeElement as HTMLElement);
 await user.keyboard('{Escape}');expect(close).toHaveBeenCalledTimes(1);expect(screen.getByText('Open')).toHaveFocus();
});
it('keeps a rejected close request open',async()=>{
 const close=vi.fn();render(<Sheet open title="Draft" onRequestClose={close}><button>Save</button></Sheet>);
 await userEvent.keyboard('{Escape}');expect(close).toHaveBeenCalled();expect(screen.getByRole('dialog')).toBeVisible();
});
it('restores background interaction when a confirmation and its parent close together',async()=>{
 function Example(){const [parent,setParent]=useState(false),[child,setChild]=useState(false);return <><button onClick={()=>setParent(true)}>Open parent</button><Sheet open={parent} title="Parent" onRequestClose={()=>setParent(false)}><button onClick={()=>setChild(true)}>Confirm</button></Sheet><Sheet open={child} title="Confirmation" onRequestClose={()=>setChild(false)}><button onClick={()=>{setParent(false);setChild(false);}}>Save both</button></Sheet></>;}
 const user=userEvent.setup();const view=render(<Example/>);await user.click(screen.getByText('Open parent'));await user.click(screen.getByText('Confirm'));await user.click(screen.getByText('Save both'));expect(screen.queryByRole('dialog')).not.toBeInTheDocument();expect(view.container.inert).not.toBe(true);expect(screen.getByText('Open parent')).toHaveFocus();
});
